# ADR-0132 — What a model is actually paid per forward it ran, and why the gap is liveness before it is price

* Status: **PROPOSED 2026-09-17; the end-to-end shadow (§6) built the same day** on
  `feat/palw-exec-lane-and-validator-retirement`. No consensus rule, parameter or fingerprint moves.
* Operator's direction, in the operator's words: "価格式だけ公平ではなく … 実際に支払われた MSK まで含めた end-to-end
  の actual MSK / attempted Economic CCU をモデル間で可能な限り一致させること"; "Final 率差の原因が protocol 上の
  不公平なのか、単なる runtime / artifact availability 問題なのかを分離"; "compute cost と data movement /
  availability cost は分離"; "モデル名ごとの手書き固定倍率は禁止"; "5 年後にモデルが 10 種類になっても破綻しにくい案".
* Builds on: [0131](0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) (economic
  compute, the three bases, op 185), [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  (the panel split and the work price), [0130](0130-bps1-is-hardened-before-it-is-widened.md) (shadow first),
  [0072](0072-the-attempt-is-a-function-of-the-execution.md) (one execution, two lotteries),
  [0071](0071-hash-coupling-and-the-remainder.md) SA-3 (`palw_capability_bound`), [0117](0117-a-draw-is-one-forward.md),
  [0098](0098-a-panels-coverage-of-one-claim-is-a-number.md), [0107](0107-share-growth-is-counted-at-final.md).

## 0. The sentence this ADR is

**On testnet-11 today a dense-tier claim is paid, in expectation, 0.60 MSK per 10⁹ MAC-equivalents its producer
ran, and a hybrid claim is paid nothing — not because the price is wrong but because no seat that can run the
hybrid ever sits on its panel; the price, once the hybrid licenses at all, pays it 2.8 × more per forward than the
dense tier on today's leaf basis and exactly the same on an attempted-compute basis; so the order of work is
liveness (a panel that can run what it judges), then the basis (attempted compute at a fixed rate), and never a
per-model multiplier.**

## 1. The path a forward takes to a payout (read without changing code, 2026-09-17, DAA 5,774–5,787)

1. **A draw is one forward and two lotteries** (ADR-0072/0117, `kaspad/src/palw_producer.rs::produce_one`). The
   class ticket `H(execution) ≤ class_target` admits with `p_class = (T + 1) / 2¹²⁸`; a winning execution then
   faces the network draw, the Layer-0 digest of the header against `bits`, with `p_net = (target₂₅₆ + 1) / 2²⁵⁶`.
   Neither lottery is searched: a lost bucket is a lost inference. **Only the class lottery is credited**:
   `claim.pwu = ⌊2¹²⁸/(T+1)⌋ × leaves` counts class draws; the forwards the network draw discards buy no weight
   and no pay.
2. **The hybrid producer's counter** (`getPalwNodeStatus` on the ibm `.t11` node, bond `:0`, the only
   `Qwen3.6-35B-A3B` producer): `979 draws this run, 202 produced, 777 won the class ticket and lost the network
   draw against bits; class ticket p = 9.980e-1 per draw`. So `1 / (p_class · p_net) = 4.85` forwards a block
   while the chain credits 1.002. `bits` at the tip is `0x207fffff` — the floor of the difficulty, `p_net = 0.5` —
   and averaged `p_net ≈ 0.21` over the run. `p_net` is the same for every class at a moment (one `bits`, one
   digest rule), so it does not move the cross-model ratio; it multiplies every class's attempted compute by 2–5.
3. **What one hybrid forward moves.** The same node logs `this draw read 9,4xx–9,9xx MiB from storage (no mapped
   class holds a residency: the page cache decides)` for every draw: the 23.9 GB Q4 GGUF is not resident on a
   23 GB host running two nodes (13 GB used). Compute per forward is 18.1 G MAC-eq (ADR-0131); data moved is
   ~9.7 GiB. **These are two costs, not one**, and the second is the host's, not the graph's.
4. **Acceptance → panel.** A claim binds its five seats at the first chain block past `accepted + 20`
   (`anchor_delay`), the receipt window is 600 DAA, a panel that does not license is redrawn once at ~+620 and
   the claim voids `receipt_timeout` at ~+1,242 (`PALW_RC_WINDOWS_V1`; the live medians below agree).
5. **The draw is blind to what a seat holds** past no fence: `palw_bond_may_judge_class_v3(…, require_production
   = palw_capability_bound_at(anchor))`, and `palw_capability_bound` is `None` on testnet-11, so a bond judges
   every class it *declared* — every genesis bond declares all three. The executor and its operator are excluded
   from their own panel (ADR-0071).
6. **What seats do.** On the ibm seat's last 400 k log lines: `717 × "interval seat — drew [4] of N interval(s),
   0 opening(s) held"` → `748 × "asked the executor for interval(s)"` → `9 × "holding a served opening"` →
   `504 × "no material held after 2–4 DAA — replaying the anchor's job"` → `497 × "licensed by replay … 6,508,520
   leaves replayed, 20.1 s mean (12–61 s)"` → `527 × "Valid"`, `65 × "Unavailable"`, `0 × "Incapable"`. The
   sampled-interval path (ADR-0098) never verified anything: **every licence on testnet-11 is a full replay by
   every live seat**, so a claim's verification compute is `live seats × the attempt's job`, not the four
   intervals the design prices.
7. **Final → MSK.** Below DAA 6,001 every `Final` names its producer the whole escrow — coinbase outputs of exactly
   `275,628,448,680` sompi (2,756.28 MSK) at DAA 5,758 (×2), 5,763, 5,765 and 5,766 on the selected chain — and
   no seat is paid. From 6,001: escrow 3,200.85 MSK × the class's leaf price (dense tier 73.7 %, hybrid 29.8 %,
   unit = the 1 ‰ 27B) → 80 % producer / 20 % panel pool (unused seats' shares → reserve; the priced-away
   remainder never minted). Claims whose attempt block was merged below the deep fence hold `escrow 0` (paid at
   acceptance, ADR-0058 B-1 before 6,000): 111 dense, 81 hybrid in the window.

**The live window** (1,464 distinct attempt-lane claims from the eight genesis bonds' executor and seat views,
DAA 5,774 → 5,785):

| | Qwen2.5-1.5B@512 (dense) | Qwen3.6-35B-A3B (hybrid) | floor |
|---|---|---|---|
| accepted in view (accepted DAA) | 919 (3,542–5,629) | 500 (4,703–5,774) | 45 |
| provisional / bound / licensed / Final / voided | 10 / 424 / 117 / **48** / 320 (`receipt_timeout`) | 20 / 480 / **0 / 0 / 0** | 0 / 18 / 27 / 0 / 0 |
| redrawn (second panel) | 493 | 282 | 19 |
| accepted → first bind / redraw / licence / Final / void (median DAA) | 20 / 641 / — / **2,017** / 1,242 | 20 / 641 / — / — / — | 20 / — / — / — / — |
| producers | premine `:2` + 4 external operators | premine `:0` only | premine |
| seats drawn (bond index : panels) | 0–7 + 4 external, uniform | `1–7` uniform, **never `:0`** | |
| licence rate (licensed + Final) / accepted | 18.0 % | **0 %** | 60 % |
| Final rate, terminal claims | 48 / 368 = **13.0 %** | — | — |
| paid so far (pre-6,001 rule) | 48 × 2,756.28 = **132,301 MSK**, producers only | **0** | 0 |

**Why the hybrid never licenses — three facts, none a price.** The only host holding its artifact (`:0`) is its
only producer and is excluded from its own panels; bond `:1` (same host, no hybrid artifact) is drawn on 344 of
482 hybrid panels while its node reports `Chain participation held: state=ibd-running. Not mining, not
attesting` (a dead seat, filing nothing); bonds `2–7` never file a hybrid receipt (their hosts are memory-bound,
ADR-0112). Three of five seats must replay a 24 GB artifact within 20 h; zero can. The claim then waits the
whole window twice — `Incapable` is never filed here, and where it is filed it counts toward nothing and shortens
nothing (ADR-0065 D4) — and voids after 41 h with the producer's reservation held throughout. **The dense tier's
own 13 % `Final` rate is the same disease at lower dose**: one seat (`:0`) replays every dense claim it is dealt
in 20 s; the rest of the fleet does not keep up, and 87 % of terminal dense claims time out.

## 2. The end-to-end numbers, per accepted claim (expectation; `p_net` at the run's 0.206)

| metric | dense | hybrid | note |
|---|---|---|---|
| `expected_attempts_q32` (class) | 1.495 | 1.002 | ADR-0131 §7 |
| network draws per class win, `1 / p_net` | 4.85 | 4.85 | one `bits` for all |
| forwards per accepted claim | 7.24 | 4.86 | `1 / (p_class · p_net)` |
| attempted economic CCU per accepted claim | 601.6 G | 87.8 G | × the draw job (83.1 G / 18.06 G) |
| data moved per forward | not measured (A16, resident) | ~9.7 GiB | host property |
| panel verification CCU per bound claim (today: full replay × live seats) | ≤ 5 × 83.1 G = 415 G | ≤ 5 × 18.06 G (never run) | design: 4 intervals a seat |
| P(Final) per terminal claim | 0.130 | 0 | |
| **producer actual MSK / attempted 10⁹ CCU, pre-6,001** | 0.130 × 2,756.28 / 601.6 = **0.598** | **0** | gap ∞ |
| producer actual, 6,001 leaf basis (× 0.737 × 0.8) | 0.130 × 1,886.36 / 601.6 = **0.408** | 0 (would be **1.134** at the dense tier's Final rate) | price gap 178 % |
| producer actual, 6,001 economic-attempted basis, dense-tier unit | 0.130 × 2,560.68 / 601.6 = 0.553 | (0.553 at equal Final rate) | price gap 0 % |
| panel actual MSK / verification CCU, pre-6,001 | 0 | 0 | seats unpaid below 6,001 |

**Decomposition of the realized gap** (`actual_A₃₆ / actual_A₂₅`):
`= [price per claim ratio 954.96 / 2,357.95 = 0.405] × [attempted CCU ratio 601.6 / 87.8 = 6.85] × [Final-rate
ratio 0 / 0.130 = 0]`. The first two terms multiply to 2.78 (the 178 % price gap in the hybrid's favour); the third
is zero. **Today 100 % of the realized distortion is the liveness term**; the price term is what remains once the
hybrid licenses, and it flips the sign: the hybrid would be over-paid per forward, not under-paid.

## 3. Hypotheses for why a gap remains, each with what would falsify it

* **H1 — the panel cannot run the hybrid** (topology): the executor exclusion plus one artifact host. Falsified
  if a hybrid claim licenses with the fleet unchanged. It has not, in 500 claims.
* **H2 — the draw does not price availability**: declared ≠ resident (§1.5). Falsified if bonds without the
  artifact stop being dealt hybrid panels; today they are dealt 344–352 each.
* **H3 — verification is 5 × a full replay**, not 4 intervals: the opening protocol is dead (748 asks, 9 openings,
  0 verified; the interval server refused 22,202 requests as `not-held`). Falsified if openings verify; none did.
* **H4 — the leaf basis prices tiles, not compute**: ADR-0131 (86 % / 178 %). Falsified by the CCU table's
  calibration; the residual is the cost table's (§7).
* **H5 — the fork-choice factor floors the draws**: 1.495 read as 1 (ADR-0131 §7).
* **H6 — the network draw wastes 2–5 forwards a block for everyone**: class-neutral, but it multiplies the absolute
  attempted compute and the hybrid's I/O (4.85 × 9.7 GiB a block). Falsified if `p_net` differed by class; it cannot.
* **H7 — data movement is read as compute**: a 2-minute hybrid draw is 9.7 GiB of page-ins on a 23 GB host, not
  18 G MAC-eq of arithmetic. Falsified on a host where the artifact is resident (ADR-0131 §7 asks for one).
* **H8 — the 600-DAA window turns every dead panel into 41 h of held collateral**, so a class with weak panels
  loses claims before any price applies (ADR-0130's next ADR: a shorter liability lifetime).
* **H9 — merged blocks below the deep fence were paid at acceptance** without licensing (`escrow 0`): a
  class-neutral leak that ended at 6,000.

## 4. Proposals, compared

Twelve attributes each. "Consensus" = changes a rule a block is judged by (needs a fence, never retroactive).

| | A. `EconomicAttempted` as the future basis | B. Snapshot the claim's economics at acceptance | C. Unit → fixed rate (`min(escrow, attempted_ccu × rate)`) | D. Fixed pool shared by CCU ratio | E. Producer CCU and panel verification CCU priced apart | F. Panel liveness (capability by evidence, `Incapable` fast path, availability cost) | G. Fewer full replays (openings that work; sampled verification; optimistic licensing) | H. Absorb Final-rate gaps in the price | S. One lottery (drop the network draw for attempts) |
|---|---|---|---|---|---|---|---|---|---|
| fixes | price ∝ forwards actually run (class draws × network draws × draw CCU) | pay independent of later retargets, registrations, unit moves | a 1 ‰ registration can no longer lower every class's pay; heavy models cap at escrow | emission neutral by construction | panel pay follows verification cost, not the producer's | the hybrid licensing at all; dead seats not dealt | 5 × replay → ~1–2 × sampled; seat capacity ×3–5 | nothing real | 79 % of forwards not thrown away |
| distortion addressed | H4, H5, H6 (absolute) | drift between accept and Final | unit manipulation (ADR-0131 §1) | none of the live ones | future (when verification ≠ job) | **H1, H2, H8** | **H3**, and H1 by capacity | — (it masks H1) | H6 |
| consensus-critical | yes (basis at a fence) | yes (claim record fields) | yes (the rate is a fenced parameter) | yes | yes (split rule) | F1/F2 yes; F3/F4 no | G1 no; G2 no (design exists); G3 yes | yes | yes, large |
| migration | new claims only; fence height | new claims only; state version bump | none beyond the fence | none | none | none (F1 reads existing facts) | none | — | header/template rules |
| auto-extends to new models | yes (graph-derived CCU) | yes | yes; a heavier-than-cap model is paid whole and signals a re-pin | yes but dilutes others | yes | yes | yes | no (per-class rates = multipliers by another name) | yes |
| attack surface | a class can inflate its own draws only by a tighter target it does not control | none new | the rate is a governance constant; a heavy registrant gains nothing from others | registrant floods CCU to dilute | seats over-report verification? (measured from the rule, not self-reported) | F1: a bond must show a receipt/production on the class — cheap, honest; F2: Incapable is unsigned-cost → must not be slashable, only informational | G3 shifts security to the court: a lie is caught by sampling w.p. ADR-0098's number, not 3-of-5 | encourages failing to license | removes the `bits` ceiling on block rate → class targets must hold the cadence alone |
| liveness impact | none | none | none | none | none | **large positive** | positive (seat capacity) | negative (rewards silence) | positive (fewer wasted forwards) |
| panel load | none | none | none | none | none | less (no hopeless duties) | **much less** | none | none |
| producer load | none | none | none | none | none | less waiting | none | none | **÷ 2–5** |
| MSK emission | unchanged (remainder never minted) | unchanged | unchanged | unchanged (by construction) | unchanged | unchanged | unchanged | unchanged | unchanged |
| implementation | small (shadow exists) | small–medium | small (one fence, one constant) | small | medium (meter first) | F1 small (reuse `palw_bond_produced_on_class` + a receipt fact); F2 small; F3 node-side medium | G1 investigation; G2 exists; G3 medium | trivial | large |
| rollback | fence height → `never` before it fires | same | same | same | same | same | node-side: config | — | hard |

**What this ADR recommends, in order.**

1. **F first, without a fence, this week**: put the hybrid artifact on three seat hosts that are not its producer
   (or make bonds `2–7`'s hosts able to page it — a 24 GB Q4 file needs a 32 GB host or NVMe page cache); fix the
   `.t11b` node stuck in IBD; find why openings are `not-held` (G1). None of this is consensus, and it is the whole
   of today's realized gap.
2. **Measure end to end in shadow (§6, built)**: actual MSK paid, attempted CCU with both lotteries, verification
   CCU, lifetimes, per class; the fleet's artifact I/O beside its compute. Then the gap has a number that is not
   infinity.
3. **One fence, when the shadow says so** (ADR-0131 Decision 3's height): **C + A + B + F1 + F2** together — a
   claim snapshots `(expected_attempts_q32, network_expected_attempts_q32)` at acceptance (B), is paid
   `min(escrow, draw_ccu × both × rate)` (A + C) with `rate` a fenced constant calibrated from the shadow so the
   heaviest *live* class at that height is paid its escrow whole, 80/20 unchanged (E deferred until verification
   CCU is measured); a bond is dealt a class it has produced on **or filed a Valid/Invalid receipt on** within
   `capability_evidence_daa` (F1, an extension of ADR-0071 SA-3's `palw_capability_bound` that verify-only
   operators can satisfy); a panel whose `Incapable` answers make quorum unreachable is redrawn at once, and a
   class with no drawable panel voids at bind as `NoCapablePanel` (F2) — a class-liveness fact ADR-0107 can read.
4. **Not H.** A per-class multiplier for a low `Final` rate pays for claims that never finalize — i.e. nothing —
   and, once the class finalizes, over-pays it. Liveness is fixed as liveness.

**Three horizons.** *Now*: 1–2 above (no consensus). *Shadow → fence*: 3, after weeks of shadow data on a fleet
where the hybrid licenses. *Ideal*: **S** (one lottery) with **G3** (optimistic licensing on sampled audits) and
ADR-0107 (share by `Final`) — §5.

## 5. How the protocol should be simpler (the author's own proposal)

Today a forward runs two lotteries, five seats each re-run it whole, and a claim waits up to 41 hours for a
panel that may hold nothing. Three cuts, each measured on this chain:

* **One lottery.** The class targets already fix every class's block rate (`Σ share = 1`, retargeted by census).
  The network draw against `bits` adds nothing to that — it only discards `1 − p_net` of winning executions
  (79 % on the hybrid's run, 50 % at the difficulty floor) and credits none of them. Let the class target carry the
  cadence: `T_c = MAX · share_c · (block interval / class forward interval)`, retargeted as now, and `bits` becomes
  a derived witness of the class targets. A block costs one forward; the 9.7 GiB the hybrid moves per draw is moved
  once a block, not five times. Consensus-critical and large (header rules, templates, the heartbeat lane's
  separate `bits`); the pay-off is measured: 777 of 979 forwards.
* **Optimistic licensing over sampled audits.** ADR-0098 already says the panel's protection against a one-leaf
  lie is 6.5 % at 300 tokens and five seats; the court is what convicts. So make the licence what it already is in
  effect — *nobody proved a fault in the window* — with each seat replaying only its drawn intervals (a fixed
  fraction of the job, ADR-0098's number), and a single full replay a claim assigned by the draw to ONE seat.
  Verification compute drops from `5 × job` to `≈ 1 × job + 4 × intervals`, a panel that holds no artifact can
  say `Incapable` and be replaced the same span, and a class licenses as long as one capable seat exists — the
  hybrid would have licensed on this fleet. What it costs: the licence no longer carries three signatures over the
  whole execution; the reservation and the court carry the security, as they do for the free-prompt lane.
* **Pay compute at a rate, not a share of a class.** `min(escrow, ccu × rate)`: no unit, no heaviest class, no
  per-model number anywhere; a new model registers, runs in shadow until its CCU and its licence rate are
  measured, and is then paid by the same constant as every other. Ten models in five years change nothing here.

## 6. What is built (shadow, node-local)

Nothing consensus reads; the fingerprint does not move.

* **The network draw, priced from `bits`** — `palw_network_expected_attempts_q32_v1(bits) = ⌊2²⁵⁶ · 2³² /
  (target₂₅₆ + 1)⌋` (`consensus/core/src/palw_economic_compute_v1.rs`): the second lottery a winning forward
  faces (2.0 at the difficulty floor). ADR-0131's attempted basis now reads `class draws × network draws × one
  draw's job`, and the census carries the tip's `bits`.
* **The end-to-end ledger** (`consensus/core/src/palw_economics_ledger_v1.rs`, pure): an observation of every
  attempt-lane claim in the state (phase marks, escrow, the duty row's seats and credits); a row that keeps its
  first sight (class draws, network draws of the accepted block, the draw's compute, leaves) and fills its
  lifecycle marks once; the payout **as the rule in force at the claim's `Final`** derives it — the escrow
  whole below the fences, past them the leaf price against the unit, 80/20, credited seats, reserve, and the
  priced-away escrow burned (`palw_ledger_payout_v1`, the fold's own functions); per-class totals with the
  three actual rates the operator named (`producer / attempted`, `panel / verification`, `total / attempted`),
  `Final` and licence rates, mean waits (bind, licence, `Final`, void), mean draws (class and network); and
  proposal C's `min(escrow, attempted × rate)`.
* **The node's recorder** (`kaspad/src/palw_economics.rs`): rows in the meta database
  (`PalwEconomicsLedgerV1`, kept past the chain's retention, a schema marker that drops rows of another
  layout), refreshed every 60 s on its own thread and before every op 185 answer; the network draws of a row
  from its accepted block's header; the payout rule from `Params` at the `Final`'s height.
* **The node's telemetry**: per class, what THIS node did — the producer's draws, class wins, blocks, forward
  milliseconds and the MiB the process read from storage during each draw (ADR-0112's number, now counted);
  the seat's whole-job replays with their milliseconds and leaves, receipts by verdict, openings held.
* **Op 185 `getPalwClassEconomics`**, extended (never shipped, so its layout may still change): the tip's
  `bits` and network draws; per class a `ledger` block (counts, sompi paid to producers and seats, reserve,
  burned, attempted / `Final` / verification compute, the three actual rates, licence and `Final` rates,
  waits, mean draws) and a `telemetry` block. **`misaka palw economics`** prints the end-to-end table, the
  actual gaps (an infinite gap names the class paid nothing), this node's own work, and each class's
  `Final`s priced under every basis at the class's ACTUAL `Final` rate — `current` vs `EconomicJob` vs
  `EconomicAttempted` vs `EconomicRate (C)` side by side; JSON schema `misaka.palw.economics.v2` with
  `attempted_ccu`, `final_ccu`, `verification_ccu`, `actual_*_msk`, `msk_per_attempted_ccu`,
  `msk_per_final_ccu`, `panel_msk_per_verification_ccu`, the counts and rates, `avg_expected_attempts_q32`,
  `avg_claim_lifetime_*`, `avg_bind_wait_daa`, `artifact_bytes_fetched_mib`.
* **Not built**: artifact cache hit rate and fetch latency as such (the MiB read from storage per draw is the
  measured proxy; a resident artifact reads ~0 MiB, a non-resident one its working set); the paid amounts of
  claims the recorder first saw after their `Final` are derived, not observed (the duty row is gone by then,
  so their credited seats read as none); ADR-0091's buyback slice; a recorder that survives a node that
  never ran it (the window starts when the node does).

## 7. Implementation record

* 2026-09-17, `feat/palw-exec-lane-and-validator-retirement`: the read (§1–2) and the proposals (§4–5);
  then §6 on the same day. Tests: `adr0132_*` in `palw_economic_compute_v1` (the network draw from `bits`:
  the floor's coin flip, monotone in the target, saturation, the ratio invariant across `bits`),
  `palw_economics_ledger_v1` (the payout by the rule at the `Final` with the emission identity; a row's
  first sight and marks; the actual rates as a property of the class across the 100/0, 0/100, 50/50, 90/10
  and 10/90 mixes; a `Final`-rate gap and a licence-rate gap read as pay, not price; targets and `bits`
  move attempted compute, not pay; a class no panel can run pays nothing and verifies nothing, and one
  operator down reads as the licence rate it costs; a heavier model moves the unit but not a rate),
  `kaspad::palw_economics` (the store round-trips, upserts and drops a foreign layout; the telemetry counts
  per class) and `misaka-cli::palw_economics` (the end-to-end reading is what was paid; the four-basis
  table). Verification on `feat/palw-exec-lane-and-validator-retirement` (2026-09-17): consensus-core 2,142 /
  consensus (evm) 305 / kaspad 98 / misaka-cli 176 / rpc-core 148 / rpc-service 1 / database 8 / integration
  `rpc_tests::sanity_test` 1 — all green; clippy `-D warnings` over the eleven crates clean;
  `shipped_presets_have_pinned_fingerprints` unmoved (`ab4e7b9c…`).

* 2026-09-17 (later the same day), **Protocol Upgrade C behind a dormant fence** — `Params::palw_economic_payout:
  Option<PalwEconomicPayoutV1 { activation, rate_sompi_per_giga, panel_share_alpha_permille,
  panel_share_min_permille, panel_share_max_permille, cap_utilization_max_permille }>`, `None` on every shipped
  preset (the testnet-11 fingerprint `135b6ee0…` does not move), hashed `Some`-only with its five numbers,
  refused without `palw_model_registry` and `palw_panel_economy` at or below its height, and by name in the
  fork-id arm (`palw_economic_payout`, the devnet's numbers). What it does: **C + A + B** together — a
  model-class claim accepted past the fence writes a `claim_economics` row (state tail `0xAB`, delta
  discriminant 53, rooted once any exists, byte-identical below): the class draws at its target (Q32), the
  network draws at the carrying block's `bits` (Q32, one where no header was at hand), one draw's job and one
  seat's verification compute from the registry's work for the class, the rate then, and the panel share
  `clamp(α·C_V / (C_P + α·C_V), S_min, S_max)` with `C_P` the attempted compute and `C_V = seats ×
  verification` (the operator's rule; `α`, the bounds and the ceiling are the fence's, the same for every
  class). Its `Final` is paid `min(escrow, attempted × rate)`, the panel that share (per seat fixed, the
  uncredited to the reserve), the rest never named; the row leaves with the `Final` or the void. The floor takes
  no row and folds byte-identically; a claim accepted below the fence is paid as before whatever the fence says
  at its `Final`. **The cap ceiling as a rule** (ADR-0133 Fence 3): each registry boundary records a class's
  cap utilization (`attempted × rate` against the escrow a claim of the boundary block holds) and a class over
  the ceiling is not stepped to `ACTIVE`; an `ACTIVE` one falls back to a tenth. **Not in this fence**: S (the
  single lottery, Upgrade B — its own ADR, ADR-0133 Fence 2) and H (a licence-rate correction, which is a
  per-class multiplier by another name and stays rejected); "budgeted admission" is Upgrade A's admission-derived
  shares, already in force under the registry. Surfaces: op 186 carries `capUtilizationPermille`; the shadow
  ledger prices a snapshotted `Final` by its snapshot (`PalwLedgerEconomicRuleV1`; the row keeps the chain's
  numbers over the recorder's reading); `--palw-economic-payout-devnet <daa>` arms it on a private devnet with
  `PALW_ECONOMIC_PAYOUT_DEVNET_V1`. Tests: `adr0132_*` — the module's own (price, network draw, share bounds,
  cap, borsh layout), the fold's four (snapshot and Final at the rate against the same chain below the fence;
  the snapshot fixes the price; the floor and a void; cap utilization recorded and a saturated class not
  activated), the registry's step, the ledger's mirror, the params' fence — 21 green on consensus-core.
  **Arming**: testnet-11 at 6,001 beside the registry, after Upgrade A's end-to-end drill passes, with `rate`
  calibrated from the shadow so the heaviest live class sits under the 80 % ceiling (a class paid its escrow
  whole reads as saturated and would not activate).

* **Armed for testnet-11 (prepared 2026-09-17 night on `wip/arm-6001-registry-payout`, merged only on the
  operator's go):** `palw_economic_payout` at the 6,001 flag day beside the registry, with
  `rate = 900,000,000 sompi per 10⁹ MAC-eq` (9 MSK per G MAC-eq), `α = 100 ‰`, the panel share between
  100 ‰ and 300 ‰, the cap ceiling 800 ‰. Calibration from the shadow's numbers at DAA ~5,780 (§1–2,
  ADR-0131): the dense A16 row's attempted compute is 1.495 class draws × 2.0 network draws (the difficulty
  floor's `bits`) × 83.1 G MAC-eq ≈ 248.5 G a claim — the heaviest live class — priced at 2,237 MSK of the
  3,200.85 MSK escrow (69.9 %, under the ceiling with room for a retarget to ~1.7 draws); the hybrid's
  36.2 G a claim at 326 MSK (10.2 %). At `α = 0.1` a five-seat full replay gives the panel 14 % (dense:
  `C_V = 5 × 83.1 G`) and 20 % (hybrid: `5 × 18.1 G`) — the operator's "light verification ~10 %, heavy
  20–30 %" band. What the price leaves (30 % of the dense escrow, 90 % of the hybrid's) is never minted.
  The fingerprint moves to `32c2e8e3…`; the fork id does not.

## 8. Number hygiene

0132 was free when written; the next free number is 0133.
