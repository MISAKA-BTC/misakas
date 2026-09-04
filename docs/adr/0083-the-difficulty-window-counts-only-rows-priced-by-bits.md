# ADR-0083: The difficulty window counts only rows priced by `bits` — heartbeat emitters are not work

**Status:** PROPOSED (2026-09-04), implemented on `palw-daa-bits-priced-rows` with the fence
dormant on every preset; **not armed anywhere**. How it reaches testnet-11 — a scheduled
`ForkActivation` on the live chain (fingerprint moves, genesis does not; a flag-day restart of the
seats, no wipe) or folded into a Relaunch 5g re-mint — is the operator's decision, recorded in §5.
**Builds on:** ADR-0060 Decision 1 (the heartbeat lane), ADR-0066 Decisions 1–3 (the lane's price
leaves `header.bits`; its slot rule; attempt blue work is a constant), ADR-0071 (`bits` keeps the
block-interval control that target-freezing removed), ADR-0072 (one inference, one draw: both the
class ticket and the Layer-0 digest are functions of the execution commitment).
**Amends:** ADR-0066 Decision 1, one sentence — *"Heartbeat headers carry the global expected bits
like every other lane, so they enter the difficulty window as ordinary rows and F1 and F3b both
disappear."* They carry the bits and they enter the window; they must not be **counted** as rows the
retarget slows down. F1 did not disappear; it moved from the lane's price to the lane's count.

## 1. What was measured (testnet-11 Relaunch 5f, genesis `ad30b5cb…`, 2026-09-04 02:00–04:10Z)

Read from node0 over wRPC (`getBlocks` from genesis, 829 blocks, sink DAA 826), reproduced
independently by a second session against its own node's copy of the chain (841 blocks):

| quantity | value |
|---|---|
| block time | 120 s — `DnsParams … with_two_minute_cadence()`, not the preset comment's 10 s |
| selected chain | genesis + 255 heartbeats (algo 8), interval 122 s at p10 = p50 = p90 |
| non-chain rows | 569 heartbeats, 3 attempt blocks (algo 6) at DAA 40 / 80 / 226 |
| rows per slot | 3.1–3.4: the chain heartbeat plus 2–3 sibling heartbeats (`PALW_HEARTBEAT_MAX_PER_MERGESET` = 4) |
| heartbeat emitters | 5 (two per host on ibm and .113, one on seat2) |
| `bits` | `0x207fffff` (p = 0.5) to DAA 150, then halving every ~50 slots: `0x1f65bd04` (p = 1.5e-3, difficulty 320.7) at DAA 826 |
| floor class | target 7.9e-5 · u128::MAX = 22 · 7,708 / 2³¹ to the digit; facts available, budget 22, produced 0 |
| floor chance per draw | 7.9e-5 × 1.5e-3 ≈ 1.2e-7 — ~18 days per block per node at 5 draws/s, halving every two hours |

Emulating `SampledDifficultyManager::calculate_difficulty_bits` over the last 264 rows in mergeset
order with `target_time_per_block` = 120 s predicts the next chain block's bits within 2–3 % at four
consecutive chain blocks (DAA 815/819/821/823); with 10 s the same emulation predicts easing.

## 2. The mechanism

`new_target = average_target · measured_span / (target_time_per_block · rows)`. The window's span is
real elapsed time; its row count is not real work. One heartbeat emitter per slot is exactly on
pace. Every additional emitter adds a row per slot **without adding time**, and by ADR-0066
Decision 1 its rows are priced by the constant 2²⁴, so the tighter `bits` costs them nothing. The
retarget therefore reads "three times too fast", tightens `bits` ×3 per window, and only the lanes
priced by `bits` — the floor and every model class, whose Layer-0 digest must land under
`target(bits)` — pay. The heartbeat miner's own doc has the blind spot in one line: *"the more that
run, the harder the lane's own retarget"* — after Decision 1 the lane has no retarget of its own;
the extra emitters harden the **global** `bits` instead.

## 3. Why the chain does not recover

`heartbeat_interval_ms` is one cadence after a heartbeat parent and backs off only after a
**bonded** parent: the lane runs at full cadence *because* the bonded lanes are silent, and they are
silent *because* `bits` is tight. Cutting the emitters to one stops the tightening (ratio 122/120)
and recovers 1.7 % per window — ~340 windows to undo ×320. A pause makes a gap, and a gap eases
linearly, once. Nothing an operator can do with the lane restores `bits`; the rule has to.

## 4. Decision 1 — count only bits-priced rows; an empty count answers MAX

Past a top-level fence `Params::palw_difficulty_priced_rows: Option<ForkActivation>`
(`None` on every shipped preset; Some-only in `consensus_params_id`, normalised `never() → None`,
visited by `for_each_fence`, so a scheduled height still peers — the D4 discipline):

1. `expected_duration = target_time_per_block · sample_rate · (rows whose lane satisfies
   `algo_id_is_priced_by_bits`)`. The predicate is the exact complement of the constant-target arm
   in `kaspa_pow::State::new` — today, everything but algo 8.
2. Heartbeat rows still enter the average (they carry the global bits, as ADR-0066 says) and still
   bound the span (they are elapsed time). Only the count changes.
3. A window with **no** priced row answers `max_difficulty_target` — ADR-0066's own sentence, *"a
   V2 network runs at MAX because the class lottery, not the hash target, is its throttle"* — not
   the selected parent's bits, which would pin a chain whose attempt lanes died at the bits that
   killed them. This clause is what makes arming the fence on the live chain a recovery: its window
   holds no priced row, so its first block carries MAX.
   The retarget stays multiplicative in the window's average: a window that still holds priced
   rows under tight bits eases by the priced-rate ratio per window (unit test: ×2.95 for attempts
   every third slot), and only a window with no priced row jumps. Testnet-11's window at the height
   of arming holds none — its last attempt block is at DAA 226 — which is why arming is a recovery
   there and not merely a slope change.
4. Before the fence, and on any window where every row is priced (a hash network; a V2 chain with
   the heartbeat silent), the arithmetic is byte-for-byte the legacy retarget — same min-row
   removal, same tie-break — so old and new builds agree on every header until the height.

What the rule keeps: the interval control ADR-0071 kept. Priced rows denser than the cadence still
tighten `bits` (unit test `heartbeat_rows_no_longer_tighten_the_bits_past_the_fence`, fourth case).
What it removes: the number of heartbeat emitters from the meter.

`retarget_bits_from_rows` is public and takes rows as `getBlock` reports them, so a node's own
history can be replayed through both rules — the check a second reader can run against this patch
and its author cannot.

## 5. How it ships — the operator's decision

| path | fingerprint | genesis | fleet action | when the floor can win again |
|---|---|---|---|---|
| (a) scheduled fence on the live chain, height H | moves at H (Some-only write) | unchanged | build + restart the six seats before H; a peer on the old build peers until H and forks at H | the first block at H (its window has no priced row → MAX) |
| (b) fold into Relaunch 5g with `Some(ForkActivation::always())` on testnet-11 | new | new | wipe + re-mint per the 5f card | block 1 |

Either way the announcement's known-state line stands until then. The stop-the-bleed move — one
heartbeat emitter — is worth nothing under (b), and under (a) only spares the fence's first window
from inheriting bits another 2¹⁰ tighter; it is not a fix and this ADR does not depend on it.

## 6. What this does not decide

Whether the heartbeat lane should back off when the *sibling* count is high (the emitter count is
still visible in mergeset width, and ADR-0066's finding 3a — siblings share one admissible
timestamp — is still open); and whether `min_difficulty_window_size` should count priced rows (it
counts all rows here, deliberately: a fresh chain's first 150 blocks stay at genesis bits, which on
a V2 network is MAX already).
