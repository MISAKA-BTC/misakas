# ADR-0125 — The execution lane is a second lane inside the cadence, and it widens one permit at a time

* Status: PROPOSED 2026-09-17 on `feat/adr-0124-panel-reward-and-compute-weight` (design first, at
  the operator's request: "testnet では BPS 1 から実装して最終目標を BPS 10 とする"; the shape the
  operator brought is quoted in §1). **Nothing in consensus moves for it**: the rules that decide a
  round's permits are written as pure functions with tests (`palw_execution_lane_v1.rs`, called by
  nothing), so the arithmetic is pinned before the lane that will read it exists. The lane itself —
  its algorithm id, its envelope, its slot rule, the seed ring in the PALW state, the DAG-parameter
  activation and the window rescale — is §7's list, to be built and drilled on devnet before a
  testnet-11 height is named.
* **Corrected the same day** (§9): the first text seeded a round from the ADR-0074 chain beacon.
  The operator pointed out that the current design has no beacon for block production, and it does
  not: main's attempt lottery is a function of the execution commitment alone (ADR-0072), the
  ADR-0074 walk survives only on the free-prompt receipt lane, and the retired DNS/VLT committee
  beacon is being removed with the rest of that overlay. A round is now seeded by a recent attempt's
  execution commitment, carried in the PALW state (Decision 2).
* Builds on: [0060](0060-the-liveness-doctrine.md) D1 / [0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)
  Decisions 1–3 (a bondless, fee-only, near-weightless lane with its own algorithm id, a slot rule one
  block deep, and a price that never touches `bits` — the shape this lane takes), [0072](0072-the-ticket-is-the-execution.md)
  (the class ticket is a hash of the execution commitment, and the header nonce is only a uniqueness
  field — the reason that commitment is seed material nobody re-rolls for free), [0075](0075-certification-is-a-consensus-object.md)
  (a certified family is chain state — the security domain), [0105](0105-a-heartbeat-never-turns-a-bonded-block-red.md)
  (what a second lane does to a bonded block at `ghostdag_k = 1`), [0107](0107-a-share-grows-on-work-that-reached-final.md)
  (`Final` is what is counted), [0123](0123-the-epoch-progressively-releases-unused-class-budget.md)
  (a block that advances DAA without spending an attempt slot widens the release),
  [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  Decision 4 (the bond floor an operator's identity is keyed to).
* Amends nothing yet. When §7 lands it amends ADR-0038 Decision H's frozen cadence **for the DAG's
  parameters only** — the PALW cadence (a class's expected 120 seconds between attempts) is untouched.
* Supersedes nothing.

## 0. The sentence this ADR is

**Transactions get a fast lane without the PALW work getting a fast clock: every second is a
round, a round hands `width` execution permits to bonds whose classes earned `Final` credits in the
previous scheduler epoch — drawn from a seed no producer re-rolls for free (a recent attempt's
execution commitment, never a beacon), quotas proportional to those credits and capped at 45 % per
security domain, one permit an operator a round, no domain filling a round or holding two rounds'
worth in a row — and a permit is a light, fee-only block that mints nothing and carries no PALW
work. 1 BPS is one permit a round; 10 BPS is ten permits a round, not a 100-millisecond round.**

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
> claim E の Credit = epoch E+1 から使用。… quota は epoch 開始時に決まっていても、具体的な slot winner は
> 少し前にしか分からないようにした方がいい。
>
> testnet では BPS 1 から実装して最終目標を BPS 10 とする。
>
> 現在の方式は beacon はもうないのでは (the correction of §9)

The operator's text proposed the DNS epoch beacon `R_E` as the shuffle seed. That overlay is being
retired, and this ADR takes no dependency on any beacon (Decision 2).

## 2. What exists, and what refuses the obvious design

* **The 120-second cadence is refused at construction, not defaulted.** `validate_palw_v2` rejects
  a `ConsensusV2` params set whose `target_time_per_block` is not
  `PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS`, and rejects one whose `ghostdag_k`, mergeset limit,
  merge depth, coinbase maturity and sample rates are not `BlockrateParams::new_two_minute_bps()`'s.
  A "1 BPS" that changes the cadence is a re-mint of every V2 network.
* **`ghostdag_k = 1`.** Every 120-second preset runs `k = 1`. A second lane producing a block a
  second on the same DAG makes a bonded block's anticone routinely non-empty, and at `k = 1` that
  colours it RED — ADR-0105's incident, with eight heartbeats in one seventeen-minute draw. A fast
  lane is not just a cadence; it is the DAG-shape parameter set.
* **The heartbeat lane is the shape that already works.** ADR-0066: its own algorithm id, a target
  that is a constant and never `header.bits`, a slot rule one block deep against the selected
  parent's timestamp, ε = 1 blue work against a bonded block's 2²⁰, block level zero, declared
  subsidy zero, ordinary transactions carried, at most four per mergeset unless they form a chain.
  An execution block is a heartbeat with a permit.
* **Block production has no beacon.** The attempt lane's class ticket is
  `H(execution_commitment_v3(attempt, execution_anchor))` with the anchor
  `H(network ‖ pre-pow hash ‖ class ‖ bond ‖ nonce bucket)` — a fact of the header, not a walk. The
  ADR-0074 walk (`palw_beacon_fact_of_candidate`) is read only by the free-prompt receipt lane's
  quantum lottery, and the DNS/VLT committee beacon is retired. A new lane must not revive either:
  a walk per round is a cost per second, and a beacon is a design the operator has already left.
* **A header hash is not seed material.** On an attempt block the nonce changes the hash for free —
  ADR-0072 made it a uniqueness field — and on a heartbeat-shaped block the whole header is cheap.
  Whatever seeds a round must be a value whose change costs work.
* **Every PALW window is in DAA.** `window_bind 600`, `window_receipt 600`, `window_challenge 1200`,
  `window_court 3000`, `claim_retirement 3000`, `withdrawal_delay 7500`, the epoch 1000 — at one block
  a second each becomes 120 times shorter in wall clock. A panel that replays a seventeen-minute
  draw cannot answer inside a ten-minute receipt window.
* **A block that advances DAA without an attempt widens ADR-0123's release** for every class, so
  the release's arithmetic reads a different clock the moment the lane exists.

## 3. Decisions

**Decision 1 — the execution lane is a second lane, not a cadence.** A new algorithm id
(`POW_ALGO_ID_PALW_EXEC_ROUND_V1`) with its own envelope in `palw_commitment`
(`{ round, permit_index, bond, domain, signature }`, priced by the whole-struct hash like every
envelope), declared subsidy zero, ε blue work, block level zero, not a priced difficulty row, and
a slot rule one block deep: an execution block's timestamp names a round strictly after its
selected parent's round. It carries ordinary transactions and the fees are its producer's. It
carries no attempt, no receipt, no claim: **additional PWU = 0, additional reward = 0**, exactly as
the operator wrote.

**Decision 2 — a round is a second, and its seed is a recent attempt's execution commitment.**
`round(t) = (t − genesis_timestamp) / 1000`. The PALW state keeps a ring of the last
`PALW_EXEC_SEED_RING_V1 = 4` accepted attempts as `(accepted_daa, execution_commitment_v3)`, written
by the fold where it accepts an attempt (the value the class ticket already hashes — no new
computation). A candidate's round is seeded by
`H("PALW-EXEC-ROUND-V1" ‖ source ‖ round)`, with `source` the newest ring entry accepted at least
`PALW_EXEC_SEED_LAG_DAA = 20` below the candidate's selected parent
(`palw_execution_seed_source_v1`). No record deep enough — a fresh chain, or several attempts inside
one lag — is no seed and an empty round, never a stall. Every eligible bond draws one ticket
`H(seed ‖ bond)`; tickets sort ascending and the first `width` that pass the alternation rules hold
the round's permits, in that order (`palw_execution_permits_v1`). Eligible is: a bond `Active` at the
panel floor of ADR-0124 Decision 4 whose class earned `Final` credits in the previous scheduler
epoch.

**Decision 3 — quotas are credits, capped, and one epoch late.** A scheduler epoch is one hour of
rounds. At its start each security domain's quota is its share of the previous epoch's `Final`
attempt credits — the count ADR-0107 grows share on — capped at
`PALW_EXEC_DOMAIN_CAP_PERMILLE = 450` and water-filled in exact integers
(`palw_execution_quotas_v1`: one domain holds the lane, two split it evenly because no split keeps
both under the cap, three or more are capped and re-divided until nobody exceeds it, largest
remainders take the missing permille). A domain holding 90 % of the compute holds at most 45 % of
the execution permits: the reward follows the compute; the chain's block production does not.
Credits earned in epoch `E` are usable from `E + 1`, so a producer cannot choose a claim to shape a
schedule it can see.

**Decision 4 — the security domain is the class's certified family, and the alternation is two
rules.** A class's domain is the `FamilyCertified` family it was certified under (ADR-0075) — the
same weights' quant variants are one family, so `Qwen3.6-A → Qwen3.6-B → Qwen3.6-C` is one domain,
which is what makes the rule the operator's and not a model-name check. In a round: at most
`⌈width / 3⌉` permits to one domain, and one permit to one operator (`operator_id`, the key-derived
identity of ADR-0042). Across rounds: a domain that filled its cap in round `r` holds no permit in
round `r + 1`, and at `width = 1` that is the operator's "同一 security domain 連続禁止". A round
in which no candidate passes both rules is an empty round: the DAG simply has no execution block
that second. With one live domain the lane runs at half its width — stated, not hidden (SA-4).

**Decision 5 — widening is one constant behind its own fence.** `width` is
`PALW_EXEC_PERMITS_PER_ROUND_V1 = 1`; stages 2, 5 and 10 are the same rule with a larger constant,
each behind a fence at its own height, each preceded by the two measurements the operator named:
`10 × average execution block bytes` against the fleet's bandwidth and `10 × ML-DSA-87
verifications a second` against the slowest host. Nothing in the rule changes between 1 and 10.

**Decision 6 — the DAG's parameters move with the lane, at the same height.** The lane's first
fence activates `BlockrateParams::new_seconds_per_block(1)`'s `ghostdag_k`, mergeset limit, merge
depth, coinbase maturity, sample rates, and the finality and pruning depths through `ForkedParam`
(the Crescendo shape), and multiplies every PALW DAA-denominated window by the cadence ratio
(`PalwWindowsScaleV1`: 120 at 120 → 1 second), re-arming every in-flight deadline in the crossing
block by the same factor from the activation height. The attempt lane's class targets are seeded
and retargeted per class as today, so a class still produces an attempt about every 120 seconds:
the PALW security rate is unchanged and the schedule's per-block subsidy of the attempt lane is
unchanged. ADR-0123's release reads attempt production against an epoch that is now 120,000 DAA
long — the same hours.

**Decision 7 — 1 BPS on devnet first, then testnet-11 at a height.** The lane is drilled on devnet
with the two-node drill of ADR-0068 (a chain born over heartbeats, a bond registered, execution
blocks a second between attempts, a reorg across an attempt), then armed on testnet-11 at a height
above the tip with a roll window — a flag day, at a NEW height (a fence at a height an earlier
build already schedules is invisible to the fork-id gate).

## 4. What is built here, and what is not

Built: `palw_execution_lane_v1.rs` — `palw_execution_round_v1`, `PalwExecutionSeedRecordV1`,
`palw_execution_seed_source_v1`, `palw_execution_seed_v1`, `palw_execution_quotas_v1` (integers
only, pinned by a test that no floating-point type is spelled in the file), `palw_execution_permits_v1`,
and the tests that pin: the seed is the newest commitment at least the lag deep and nothing deep
enough is no seed; the round separates seeds; quotas sum to exactly 1000, never exceed the cap for
three or more domains, cascade, and round by largest remainder; one operator a round; a domain's cap
a round; a domain that filled its cap is absent next round; a round nobody passes is empty; one live
domain produces exactly every other round; a dominant domain holds at most half a long run at width
ten; widening from 1 to 10 changes only the count. **Not built** (§7): the algorithm id and envelope,
the slot rule, the acceptance gate, the seed ring in the PALW state, the producer (`kaspad`), the
DAG-parameter activation, the window rescale and its deadline re-arm, the fee-only coinbase arm.

## 5. Security amendments, stated before the build

* **SA-1 — the seed costs work to move.** The source is an attempt's execution commitment, which
  the header nonce does not reach: re-rolling it is a new inference that must also win its class
  lottery, and a fabricated commitment that wins is a claim the panel and the court void and slash.
  An execution block's own header never enters any seed.
* **SA-2 — the lag stops parent selection, and winners are known about twenty rounds ahead.** An
  execution block chooses its parents, so a seed read at the tip could be swapped by leaving the
  newest attempt out of the parent set. Twenty DAA below the selected parent is in every candidate's
  past; a round's winners become computable once that attempt is twenty deep — the operator's
  "少し前にしか分からない", with the horizon set by the lag, not by a beacon epoch. A longer secret
  horizon would need a per-round VRF, which is a later ADR.
* **SA-3 — no lane without its DAG parameters.** Decision 6 is not separable from Decision 1: a
  build that opens the lane at `ghostdag_k = 1` re-creates ADR-0105 at one block a second.
  `validate_palw_v2` must refuse the lane's fence without the `ForkedParam` set at the same height.
* **SA-4 — one domain halves the lane.** Strict alternation at `width = 1` with one live domain
  leaves every other round empty. That is the rule doing its job — a single domain cannot chain
  blocks — and a network that wants the full rate needs a second certified family.
* **SA-5 — Sybil operators pay the panel floor.** The operator rule is keyed to `operator_id`,
  derived from the bond's key, and a bond is eligible only at ADR-0124's panel floor: `N`
  consecutive permits need `N` bonds at ten producer floors each, in `N` distinct domains for the
  domain rule besides. On the operator's mainnet numbers that is 100,000 MSK a rung.
* **SA-6 — an execution block that carries a PALW object is refused.** The lane's envelope has no
  attempt and the carriage rules refuse lifecycle objects on it, so the fast lane cannot become a
  faster attempt lane by the back door.
* **SA-7 — integers only.** A quota that two platforms round differently is a fork; the module
  spells no floating-point type, and a test holds it to that.

## 6. What does not change

The attempt lane, its class targets, its budget and its subsidy; the receipt lane; the heartbeat
(which the execution lane does not replace: a stopped chain still restarts over heartbeats);
fork choice (an execution block weighs ε); every PALW object and its carriage; every network,
until the lane's fence is armed at a height.

## 7. The implementation this ADR asks for, in order

1. `POW_ALGO_ID_PALW_EXEC_ROUND_V1`, the envelope, `check_palw_commitment_shape_at`'s arm,
   `calculate_l1_tag`'s arm, `palw_lane_blue_work_v1`'s arm (ε), `algo_id_derives_no_block_level`,
   the coinbase's zero-subsidy arm, the acceptance gate ORed with the fence.
2. The slot rule (`check_execution_round`: the block's round is strictly after its selected
   parent's) and the mergeset width bound (the heartbeat's, shared).
3. The seed ring in `PalwChainStateV2` — written where the fold accepts an attempt, rooted and
   carried only once written (the ADR-0111 shape) — and `palw_execution_permits_v1` wired into
   acceptance: the block's `(round, permit_index, bond)` must be the draw's under its selected
   parent's ring.
4. `ForkedParam` for the DAG set and `PalwWindowsScaleV1` for the PALW windows at one height;
   the crossing block's deadline re-arm; `validate_palw_v2`'s coupling refusal.
5. The producer in `kaspad` (a permit holder's block a second, from the mempool, no nonce search
   beyond the heartbeat's) and `misaka mining`'s status line.
6. The devnet drill, then the testnet-11 height.

## 8. Number hygiene

0125 was free on `main` at `6fdf6ba7` when this was written beside ADR-0124 on the same branch;
the README's residency sentence names both.

## 9. The correction

Written first with `seed = H(ADR-0074 beacon ‖ round)` and a per-anchor beacon cache. That was the
wrong source on three counts the operator's one line exposed: block production on main does not
read a beacon (ADR-0072's execution anchor replaced it for the attempt lane), the beacon walk is a
chain walk the lane would pay every second, and the operator's direction retires beacon-based
overlays rather than extending them. The rules module was corrected in the same commit as this
section, and its quota arithmetic moved from floating point to exact integers at the same time.
