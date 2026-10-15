# Misaka Claim Capacity Map — testnet-12 release `0e8ec984e` (2026-09-26)

Which model, with which bond, over how many DAA, can process how many claims — by formula (the
code, with file:line) and by measurement: the real consensus pipeline in a harness (testnet-12 as
shipped, the node's own templates, fold, stake draw and receipt assemblers, real ML-DSA-87
signatures), and the live public testnet-12 read over wRPC (`wss://misakascan.com/kaspa`,
DAA 140–200). Not a security audit.

Everything is the **shipped release** (`0e8ec984e`, fp `b8564b88…`, genesis `a27f8f44…`) unless a
row says **post-DAA-500**. Paths are relative to the repository root; `state` =
`consensus/core/src/palw_state_v2.rs`.

Harness: `consensus/src/pipeline/virtual_processor/tests/t12_claim_capacity.rs` (three `#[ignore]`d
runs, see §14). Raw measurement files are listed in §14.

## 0. The answer in one paragraph

**Qwen2.5 8k with 13,000 MSK: instant cap 2, but only 1 sustained — measured 5 claims in the first
80 DAA and 3.4 per 80 DAA at steady state (harness, licence at accept+23). With 26,000 MSK the
instant cap is 3 (T-2(a) = ⌈5/2⌉), and more bond does not help: 26k, 50k, 100k … 1M all hold 3
because T-2(a) and the class room (c_8k = 5 network-wide) bind, not collateral. Effective 80-DAA
throughput is 10.2 per bond (harness: 3 per 23-DAA cycle; 26k and 100k identical, and 39,878 MSK of
the 100k bond's 50,000 ceiling never used) and ≈ 8 on the live chain (b0, DAA 32–139: licence
latency p50 23 / max 28 DAA plus 2–4 DAA of 8k inference), falling to one usable slot (≈ 3/80) once
S2-licensed claims hold two T-2(a) slots (live from DAA ~160).** The known "6,500 / 3,225.56 = 2.01 → 2" is right for the instant; it is not the steady
state: the 24.716 MSK weight term of every licensed 8k claim stays on the bond until Final (L+121),
and two of them exhaust the 48.87 MSK of slack a 13k bond has above two full reservations.

For the floor, collateral is the only per-bond limit (no T-2(a), no room): N = ⌊C/2 / 3,200.954⌋
(2 / 3 / 4 / 7 / 15 / 39 / 78 / 156 for 13k … 1M), and each slot turns over once per licence
(≈ 21 DAA) **only if all five seats serve** (else the escrow stays to Final, ≈ 142 DAA). 13k gives
7.9 per 80 DAA with ideal seats (measured) and, with the live licence mix, 7.1 in the first 80 DAA
but 3.1 per 80 at steady state (model); the user's target of ~100 per 80 DAA from 13k is **13×**
away on the floor and **~30×** away on 8k even with ideal seats, and the network-wide seat capital caps floor
throughput at ≈ 1.0 claim/DAA (≈ 80 per 80 DAA, all producers together) long-run on the shipped rule
(≈ 1.9/DAA after V02) — while the live chain already produces ≈ 2.8/DAA, so the eight genesis seats
fill at ≈ DAA 1,000 (§6.2).

### The map at a glance (shipped rule; harness = real consensus, ideal seats; live-like = live latency and licence mix)

| model | bond (MSK) | instant cap | outstanding (sustained) | +10 | +20 | +40 | +80 | claims/10 DAA | claims/80 DAA harness | claims/80 live-like (first 80 → steady) | claims/1000 DAA harness | licence latency | limit |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| floor | 13,000 | 2 | 2 | 2 | 2 | 4 | 8 | 1.0 | **7.9** | 8 → 4.5 (measured) / 3.1 (model) | ≈ 99 | 21 (live p50 21, p90 30) | exposure (escrow E) |
| floor | 20,000 | 3 | 3 | 3 | 3 | 6 | 12 | 1.5 | 11.9 | 10.4 → 4.6 | ≈ 149 | 21 | exposure |
| floor | 26,000 | 4 | 4 | 4 | 4 | 8 | 16 | 2.0 | 15.9 | 14 → 6.5 (measured) / 5.9 | ≈ 199 | 21 | exposure |
| floor | 50,000 | 7 | 7 | 7 | 7 | 14 | 28 | 3.5 | 27.8 | 23.7 → 10.1 | ≈ 348 | 21 | exposure |
| floor | 100,000 | 15 | 15 | 15 | 15 | 30 | 60 | 7.4 | 59.6 | 50.5 → 21.7 | ≈ 745 | 21–23 | exposure |
| floor | 250,000 | 39 | 39 | 39 | 39 | 65 | 125 | 14.5 | 116 until the seats saturate | 131 → 59 | network-bound | 21–32 (carriage) | exposure → **seat capital** (§6.2–6.3) |
| floor | 500,000 / 1,000,000 | 78 / 156 | 78 / 156 | 78 / 156 | 78 / 156 | 139 / 288 | 269 / 545 | 31 / 65 | ≈ 245 / 520 until the seats saturate (at ≈ 11 claims/DAA network-wide: rel 110), then voids | | network-bound | 21–36 (carriage) | **seat capital**; carriage (3 licences/block) |
| 8k | 13,000 | 2 | **1** | 2 | 2 | 3 | 5 | 0.43 | **3.4** | 3.6 → 0.9 (model); measured `L3`: 3 in 300 DAA | ≈ 43 | 23 (live p50 23, max 28) | exposure: the 24.7 MSK weight hold |
| 8k | 20,000 | 3 | 2–3 | 3 | 3 | 6 | 12 | 1.1 | 9.1 | 7.7 → 1.6 | ≈ 114 | 23 | T-2(a) ↔ exposure |
| 8k | 26,000 … 1,000,000 | 3 | 3 | 3 | 3 | 6 | 12 | 1.28 | **10.2** | 8.3–9.9 → 1.7–1.9 (model); measured `L2` (100k): 10 in the first 80, **0 from rel ~160** (three S2 slots) | ≈ 128 | 23 | **T-2(a) 3**, room 5 |
| 8k | the whole network | — | 5 (c_8k) | | | | | 2.2 | **17.5** (two 100k bonds, measured) | | ≈ 217 | 23 | class room |
| 2M | any | 0 | 0 | | | | | 0 | 0 | 0 | 0 | — | closed (Prefetching, NM, C7 1, not activatable) |

Live producers (939,063 MSK each): b1 / b6 floor ≈ 104 / 103 per 80 DAA (production-bound, 146
slots idle-rich); b0 8k 6–10 per 80 DAA (median 8), held at T-2(a) since DAA 150 with two S2 claims.


## 1. The metrics (never one number)

| metric | meaning here | what sets it on 0e8ec984e |
|---|---|---|
| `instant_claim_cap` | claims a bond may open right now | min(N_bond, N_share, N_room) − what it already holds (§3) |
| `outstanding_cap` | unlicensed claims a bond may hold at once | the same gates at steady state — for 8k at 13k this is **1**, not 2 (§4.2) |
| claims / 10 DAA | short-term production | ≈ N / H_slot, H_slot ≈ 21–23 DAA (§4) |
| claims / 80 DAA | the comparison throughput | measured per bond in §4 |
| claims / 1000 DAA | long-run | per-bond ≈ 12.5 × (claims/80); network seat capital caps the sum (§6) |
| `licence_latency` | accept → licence | 20 (anchor delay) + anchor wait + replay/carriage (§5) |
| `slot_reuse_latency` | accept → the slot is usable again | floor: accept → escrow release (all 5 seats served) or → Final; 8k: T-2(a) slot at a k ≥ 2 licence, exposure slot at escrow release, the 24.7 MSK weight at Final (§5) |
| `panel_capacity` | what the network can process | 8k: c_8k = 5 unlicensed network-wide; floor: seat capital, ≈ 1.0 claim/DAA long-run; every class: ≤ 3 five-receipt licences per block (§6) |

## 2. All models and their per-claim reserve

### 2.1 The classes that exist on testnet-12

| class | id | registry row (work, window, cap) | lifecycle on live t12 (DAA 160) | admits claims? |
|---|---|---|---|---|
| floor (base) | `f1c5635c…` | eccu 21,657,728; vccu 30,504,896; window 2 spans; no room (base class is never gated) | Active | yes |
| Qwen2.5-1.5B graph-v7 @8192 ("8k", held short-window) | `ebf44d0a…` | eccu 1,388,926,972,416; vccu 1,390,562,722,816; window 3; static cap 5 (not read past the work target) | Probation {0} (8 ready seats) | yes (Probation's 50‰ prices the production share, not the room: `palw_model_registry_v1.rs:396`) |
| Qwen2.5-1.5B dense A16 @2,097,152 ("2M", C7; same artifact bytes as the 8k row) | `74c67e63…` | eccu 3,357,281,757,221,376; window 2,799; static cap 1 | Prefetching (0 ready) | **no** — Prefetching, NM `ClassDeadlineUnmeasured` (5 × 2,799 > window_receipt 600), cap-utilization 18,879,717‰ > 800‰ (not activatable) |

Any class added later is a registry row with its own derived work (`palw_model_registry_v1.rs:173`
window, `:229` max_inflight); §7 gives the capacity of an arbitrary added class as a function of
that work.

### 2.2 Per-claim reserve, term by term (harness `collateral_study`, live facts agree)

`reserve(c) = w_c + E` held on the producer's bond at acceptance (`state:4608`,
`palw_claim_bond_reservation_v1`), where

* `w_c = exposure_pwu_c × slash_value_per_pwu × attempts_c` (`state:30979-30983`), with
  `exposure_pwu_c = ⌊eccu_c × base_declared / base_canonical⌋` (`palw_exposure_pwu_v3`,
  `state:4002-4024`; the canonical draw IS the row's `economic_ccu_per_claim`, `state:9564-9570`; the
  basis is the floor's declared leaves 7,708 over the floor's eccu 21,657,728, `state:9629-9642`),
  `slash_value_per_pwu = 5` sompi (`consensus/core/src/palw_fp_devnet_v3.rs:547`, one value per network),
  `attempts_c = claim_pwu / eccu_c` (`consensus/core/src/palw_pwu.rs:179-184`: the class lottery's
  expected attempts, 279 on the floor at its current target, 1 on the model classes whose target is MAX);
* `E = ⌊subsidy × 720‰⌋` (`palw_claim_escrow_v1`, `state:29444`; carve `PALW_OVERLAY_WORKER_CARVE_PERMILLE_V1 = 720`,
  `consensus/core/src/config/params.rs:16579`, armed by `palw_overlay_carve` `:16372`; subsidy
  4,445.62014 MSK/block = 3.70468345 MSK/s × 120 s, `consensus/src/processes/coinbase.rs:541` and its table).
  **E is the block's miner reward, withheld** — the same for every class.

| class | exposure PWU | × svp | × attempts | **w** (MSK) | **E** (MSK) | **reserve** (MSK) | w share | 1-claim collateral (÷ 0.5) |
|---|---|---|---|---|---|---|---|---|
| floor | 7,708 | 5 | 279 | 0.1075266 | 3,200.8465008 | **3,200.9540274** | 0.003 % | 6,401.9 (13,000 floor binds) |
| 8k | 494,320,046 | 5 | 1 | 24.7160023 | 3,200.8465008 | **3,225.5625031** | 0.77 % | 6,451.1 (13,000 binds) |
| 2M | 1,194,858,841,364 | 5 | 1 | 59,742.9420682 | 3,200.8465008 | **62,943.7885690** | 94.9 % | **125,887.58** |

Live cross-check: every live floor claim reads `reservedSompi 10,752,660` + `escrowSompi
320,084,650,080`; every live 8k claim `2,471,600,230` + `320,084,650,080`; `getPalwProducerFacts`
for 2M reads `bondClaimExposure 6,294,378,856,900`.

After a licence the escrow term leaves the bond **only if** the licence door is Quorum/Coverage,
`basis_k ≥ 2`, **every** seat of the panel served, nothing Unavailable/Incapable/Sampled was carried,
and the claim was never redrawn (`palw_rcore_release_record_holds_v1`, `state:3185-3205`), either at the
licence or by a supplementary set up to `min(L + 60, bound + 600)` (`state:3487`, `:3494`). Otherwise
the escrow stays to Final. The weight `w` always stays to Final (`palw_claim_commitment_v1`,
`state:2692`). Final = `max(L + window_challenge_at(L), verify horizon)` = L + 120, first chain block
past it (`state:3296-3308`); an S2 (k = 1) licence is due no earlier than `bound + 601` (`state:3570-3585`).

## 3. Bond → concurrent cap: every limit, reduced to one formula

A claim of class `c` by bond `b` at DAA `t` is admitted by `apply_attempt` (`state:30938-31108`) and,
for the block's own attempt, first by admission (`consensus/core/src/palw_admission_v2.rs:600-701`,
where a refusal DISQUALIFIES the block, `StatusDisqualifiedFromChain`) — in this order:

| # | gate | formula | file:line | refusal |
|---|---|---|---|---|
| 1 | producer floor | `collateral ≥ 13,000 MSK` | `state:30945` | `ProducerBelowFloor` |
| 2 | class lifecycle | row admits (Probation/ActiveLimited/Active); NM class needs a measured row | `state:13025-13040`, `:12891` | `ClassNotAdmitting` / `ClassDeadlineUnmeasured` |
| 3 | **class room (N_room)**, non-base only | `owed_c + 1 ≤ cap_c`, `cap_c = ⌊(per_span_c·2³² − Σ_{d≠c} term_d)·window_c / (cost_c·2³²)⌋`, `per_span_c = ready_eff_c × 2.4e12 × 700/1000`, `cost_c = eccu_c × 5`, `term_d = ⌈owed_d × cost_d × 2³² / window_d⌉`, `owed_c` = the class's unlicensed claims network-wide (+ licensed claims under a court; C7 classes owe every claim to Final) | `state:13094-13101`, `:13226-13262`, `:14243-14290`, `:14163-14222`; `consensus/core/src/palw_work_target_v1.rs:190-208` | `PanelRoomExhausted` |
| 4 | C7 static cap (2M) | `inflight + 1 ≤ max_inflight_claims (1)` | `state:13079-13088` | `ClassInflightCapped` |
| 5 | **T-2(a) share (N_share)**, non-base only | `unlicensed(b,c) + 1 ≤ ⌈c_class/2⌉`, `c_class` = cap_c with NO other class (`others = 0`) — C7: its static cap | `state:13349-13391` | `BondClassShareExceeded` |
| 6 | **exposure (N_bond)** | `committed(b) + (w_c + E) ≤ min(⌊C·500/1000⌋, C − accuser)` | `state:31087-31106`, `:2819-2833` (room), `:2745` (committed), admission `palw_admission_v2.rs:607-701` | `AttemptExposureCeiling` (skip) / `ExposureCeilingExceeded` (block disqualified) |

"unlicensed" everywhere means *not* `palw_rcore_counts_licensed_v1` (`state:3148`): `ReceiptLicensed`
with `basis_k ≥ 2`. An **S2 licence (k = 1, one full-seat Valid) still counts as unlicensed** for
T-2(a) and the room until a supplementary set upgrades it, and is not due before `bound + 601`.

`ready_eff = min(ready operators, max(5, ⌊ΣW/W_max⌋))` over capped stake weights
(`consensus/core/src/palw_panel_v2.rs:1872-1906`; `state:13504-13534`): 8 on testnet-12's eight equal
genesis seats.

So for a pure producer (no seat duties, no accuser exposure), holding `u_c` unlicensed claims of
each class and `ℓ` licensed-not-Final claims (escrow released) and `h` licensed claims whose escrow
is held:

```
N_bond(b)  = ⌊( ⌊C/2⌋ − Σ_live (w + E·[escrow not released]) ) / (w_c + E)⌋
N_share    = ⌈c_class/2⌉ − unlicensed(b,c)                       (model classes only)
N_room     = cap_c − owed_c                                      (model classes only; shared by ALL bonds)
N_instant  = max(0, min(N_bond, N_share, N_room))                (+ lifecycle / NM / C7 gates)
c_8k       = ⌊8 × 1.68e12 × 3 / (5 × 1.3889e12)⌋ = ⌊5.806⌋ = 5  →  N_share = 3
```

No other per-bond limit exists on the attempt lane: the epoch budget is not read past the work
target (`palw_producer_v2.rs` `has_epoch_room`; live facts `epochBudgetBlocks 1` vs
`epochProducedBlocks 13` for 8k and still admitted), the class ticket is MAX for the model classes
(every attempt wins) and ~279 draws on the floor (a production cost, not a gate), and nothing limits
own-attempt blocks per DAA (attempt blocks do not move the clock on t12).

### 3.1 Measured: N_instant by the chain's own gates (harness `instant_sweep`)

Planted subject bonds (pure producers), planted Provisional claims at the true reservation, the
producer facts (`ready_to_produce_v3`, the one verdict kaspad and `getPalwProducerFacts` read) asked
after each:

| bond (MSK) | floor N_instant | stop | 8k N_instant | stop | 2M | 2M by exposure alone |
|---|---|---|---|---|---|---|
| 13,000 | **2** | exposure (6,401.9 + 3,200.95 > 6,500) | **2** | exposure (6,451.1 + 3,225.6 > 6,500) | 0 | 0 |
| 20,000 | 3 | exposure | **3** | T-2(a) "holds 3" | 0 | 0 |
| 26,000 | 4 | exposure | 3 | T-2(a) | 0 | 0 |
| 50,000 | 7 | exposure | 3 | T-2(a) | 0 | 0 |
| 100,000 | 15 | exposure | 3 | T-2(a) | 0 | 0 |
| 250,000 | 39 | exposure | 3 | T-2(a) | 0 | 1 |
| 500,000 | 78 | exposure | 3 | T-2(a) | 0 | 3 |
| 1,000,000 | 156 | exposure | 3 | T-2(a) | 0 | 7 |

(2M column: Prefetching refuses every bond; "by exposure alone" is `has_committed_room` with the
lifecycle ignored — what the bond could hold if the class opened; C7 then allows 1 network-wide.)

Theory = measurement on every cell (6,500/3,200.954 = 2.03; 6,500/3,225.56 = 2.015; 10,000/3,225.56
= 3.1 → 3 = share; 500k: 250,000/62,943.79 = 3.97 → 3).

## 4. Bond → effective claims over time

Three sources, kept apart:

* **harness (consensus-bound)** — the producer makes a claim the instant the chain admits one; every
  seat signs; licence one DAA after bind (floor) or three (8k, the live median); the real pipeline
  (§14). This is the upper bound the RULES allow, with ideal seats.
* **model (live-like)** — `capmodel3.py`: the same gates, with licence latency bootstrapped from the
  live chain and the live licence mix (floor: 90.6 % all-5 / 5.7 % k ≥ 2 but not every seat / 3.8 %
  S2 k = 1; 8k: 69 / 15.5 / 15.5 %; no supplementary receipt upgraded any live licence in 40 min of
  snapshots). Validated against the harness in §11 (F10).
* **live** — the public chain's own producers (b1/b6 floor, b0 8k, 939,063 MSK genesis bonds).

### 4.1 Floor (measured, harness run `A_floor_small`, 5 subjects on one chain, 200 DAA)

| bond (MSK) | DAA 0 | +1 | +2 | +5 | +10 | +20 | +40 | +80 | slot returns at | claims/80 DAA steady | claims/1000 DAA | limit |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 13,000 | 2 | 2 | 2 | 2 | 2 | 2 | 4 | 8 | 21, 23, 42, 44, 64, 66 … (licence + escrow release) | **7.9** | ≈ 99 | exposure |
| 20,000 | 3 | 3 | 3 | 3 | 3 | 3 | 6 | 12 | 22, 24, 44, 45 … | 11.9 | ≈ 149 | exposure |
| 26,000 | 4 | 4 | 4 | 4 | 4 | 4 | 8 | 16 | 21–24, 42–45 … | 15.9 | ≈ 199 | exposure |
| 50,000 | 7 | 7 | 7 | 7 | 7 | 7 | 14 | 28 | 21–23, 42–44 … | 27.8 | ≈ 348 | exposure |
| 100,000 | 15 | 15 | 15 | 15 | 15 | 15 | 30 | 60 | 21–23, 42–44 … | 59.6 | ≈ 745 | exposure |
| 250,000 | 39 | 39 | 39 | 39 | 39 | 39 | 65 | 125 | 21–32 (three big bonds' licences queue at 3 per block) | 116 (rel 20–100), then seat saturation (§6.3) | network-bound | exposure → **seat capital** |
| 500,000 | 78 (64 + 14: the harness makes ≤ 64 attempts per DAA per bond) | 78 | 78 | 78 | 78 | 78 | 139 | 269 | 21–36 | ≈ 245 (rel 20–100), then saturation | network-bound | seat capital |
| 1,000,000 | 156 (64 + 64 + 28) | 128 | 156 | 156 | 156 | 156 | 288 | 545 | 21–24 | ≈ 520 (rel 20–100), then saturation | network-bound | seat capital; 514 claims of the three bonds voided at their anchors after rel 110 (§6.3) |

Measured licence latency accept → licence 21 (125/135 at 100k), 22–23 when a block's three-licence
transient-mass budget queued it (§6.4); release = licence (all seats served); Final at accept + 142.

**Measured with the live licence mix and live latencies** (`L1_floor_13k_live`: each claim draws
all-five 90.6 % / three-seat V1 5.7 % / S2 3.8 % and a bind → licence delay from the live histogram;
300 DAA): 13k 8 in the first 80 DAA, **4.5/80 over rel 140–300**, one slot escrow-held from rel ~200;
26k 14 in the first 80, **6.5/80** over rel 140–300 (the model gives 6.8 / 3.8 and 13.3 / 7.9 for the
same windows — one S2 draw moves a two-slot bond by a whole slot for 600 DAA).

**Formula (floor, all seats serve):** `claims/80 ≈ N_bond × 80 / H`, `H` = accept → release ≈ 21
(harness) … 23.9 (live mean). **Live-like** (model, live latencies and licence mix): 13k 7.1 in the
first 80 DAA but **3.1/80 at steady state**, 26k 5.9, 100k 21.7, 1M 236 — each non-all-5 licence
keeps 3,200.85 MSK on the bond to Final (≈ 121 DAA) and each S2 licence keeps it to `bound + 601`,
and a 13k bond has only two slots to lose.

### 4.2 Qwen2.5 8k (measured, harness, one subject per chain unless stated, licence at bind + 3)

| bond (MSK) | DAA 0 | +1 | +2 | +5 | +10 | +20 | +40 | +80 | slot returns at | claims/80 steady | limit (instant → steady) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 13,000 | 2 | 2 | 2 | 2 | 2 | 2 | 3 | 5 | 23, then one slot every 23 | **3.4** | exposure → **exposure with 1 slot**: two licensed claims' 2 × 24.716 MSK weight exceed the 48.87 MSK slack |
| 20,000 | 3 | 3 | 3 | 3 | 3 | 3 | 6 | 12 | every 23 DAA; from rel 115 one slot waits on Final (2 ↔ 3) | **9.1** | T-2(a) (71 % of DAA) → exposure (29 %) |
| 26,000 | 3 | 3 | 3 | 3 | 3 | 3 | 6 | 12 | 23, 46, 69, … (three every 23) | **10.2** | T-2(a), always |
| 100,000 | 3 | 3 | 3 | 3 | 3 | 3 | 6 | 12 | 23, 46, 69, … | **10.2** | T-2(a), always |
| 250k / 500k / 1M | 3 | | | | | | | 12 | as 100k (formula; T-2(a) and the room are bond-independent) | 10.2 | T-2(a) |
| live b0 (939k) | 3 | — | — | — | — | — | — | 6–10 per 80-DAA window (median 8) | licence 22–28 + 2–4 DAA inference | ≈ 8 (DAA 32–139), then ≈ 3 with two S2 slots held | T-2(a), then S2 holds |

**Measured with the live licence mix and latencies** (8k mix 69 % all-five / 15.5 % three-seat /
15.5 % S2, live bind → licence draws, 300 DAA):

| run | first 80 DAA | by rel 200 | rel 200 → 300 | what froze it |
|---|---|---|---|---|
| `L2_8k_100k_live` (100k) | 10 | 18 | **0** | from rel ~160 all three T-2(a) slots are S2 licences (counted unlicensed, `share [3,3]`, escrow held) — nothing until bound + 601 |
| `L3_8k_13k_live` (13k) | 3 | 3 | **0** | from rel ~40 one S2 and one escrow-held licence fill both exposure slots |
| `S2_8k_100k` (every licence S2) | 3 | 3 (rel 120) | — | three S2 licences hold all three slots from their licence on |

The live b0 is on the same path (two of three slots S2 since DAA ~160). Under today's licence mix a
producer's 8k capacity is set by the S2 rate, not by its bond: ≈ 1.9/80 per bond at steady state
(model) whatever the collateral.

Claims/1000 DAA (harness rate × 12.5): 13k ≈ 43, 20k ≈ 114, ≥ 26k ≈ 128 per bond; the network
cannot exceed c_8k = 5 unlicensed at once (§6.1), i.e. ≈ 17 per 80 DAA and ≈ 217 per 1000 DAA for
ALL 8k producers together at the harness cycle.

**Why 13k sustains only one 8k claim (theory, then measured).** Two unlicensed 8k claims commit
6,451.125 MSK of the 6,500 ceiling, leaving 48.875. A licensed 8k claim whose escrow was released
still commits its weight, 24.716 MSK, until Final (L + 121). After the first two licence, the bond
holds 2 × 24.716 = 49.43 > 48.875, so the second slot is gone until they Final — and by then later
licensed claims replace them. Measured (`C_8k_13k`): committed 3,274.99 MSK at rel 23 = 3,225.56 +
2 × 24.716; one new claim every 23 DAA from rel 23 to 200 (23, 46, 69, 92, 115, 138, 161, 184);
10 claims in 200 DAA.

## 5. Licence latency and slot-reuse latency

### 5.1 The chain of waits (code)

accept `t` → anchor slot `t + 20` (`anchor_delay`, the panel params) → **the first attempt block at or
past the slot binds** (SW-8; a heartbeat never anchors, and the anchor block voids what it cannot bind,
`state:25641`, processor `palw_sw8_anchor_delay_for` `consensus/src/pipeline/virtual_processor/processor.rs:10653`)
→ seats replay and sign → a carrier lands the licence → `L` → Final at `L + 121` (`state:3296-3308`).
Deadlines, not waits: bind window 600, receipt window 600 (`state` params; harness dump).

### 5.2 Measured distributions

| source | class | accept → bind | bind → licence | accept → licence p10 / p50 / p90 / p95 / p99 / max (mean) | full service at licence | accept → Final |
|---|---|---|---|---|---|---|
| live t12, DAA 1–172 (316 licences) | floor | 20 (87 %), 21 | 0: 54, 1: 117, 2: 51, 3: 20, 4: 15, 5–9: 26, 10–19: 23, ≥ 20: 10 | 20 / **21** / 30 / 36 / 59 / 86 (23.9) | 281/316 = 88.9 % all 5; 21 with 3; 14 S2 (1–2 Valid) | 141–152 (72 Finals) |
| live t12 (13 licences) | 8k | 20, 21 | 2: 5, 3: 5, 4: 1, 7: 1, 8: 1 | 22 / **23** / 27 / 28 / 28 / 28 (23.5) | 9/13 all 5; 2 with 3; **2 S2** | — (first at ~178) |
| harness, all seats at bind + 1 | floor | 20 | 1–3 (the 3-per-block carrier budget) | 21 (125/135), 22, 23 | 100 % | 142–144 |
| harness, all seats at bind + 3 | 8k | 20 | 3 | 23 | 100 % | 144 |

The live floor tail (p99 59, max 86 DAA) is the collectors' claim-id-ordered licence offering that
V04 (post-DAA-500, node policy) replaces with oldest-bind-first (§12).

### 5.3 Slot-reuse latency (what the producer actually waits for)

| class | the slot | returns at | live typical | worst |
|---|---|---|---|---|
| floor | exposure (3,200.95) | escrow release: at the licence iff all 5 seats served; a later supplementary set until `min(L+60, bound+600)`; else Final | 21 (89 %) | L + 121 ≈ 142 (k ≥ 2, not all served: 6.6 %); ≥ bound + 601 (S2: 4.4 %) |
| 8k | T-2(a) share slot | a licence with `basis_k ≥ 2` (Quorum or Coverage) | 23 | S2 (k = 1): not before bound + 601 (15 % of live 8k licences) |
| 8k | exposure (3,225.56) | escrow release as floor | 23 | L + 121 / bound + 601 |
| 8k | exposure (24.716 weight) | Final only | 144 | — |

**Live consequence (b0, the only 8k producer):** claims 114 and 139 licensed S2 with one Valid each
(the full seat), so from DAA ~160 two of b0's three T-2(a) slots are held until ~DAA 735 / 760 and
b0 runs on one slot (`notReadyReason` "holds 3 unlicensed claims" from DAA 150 on).

## 6. Panel (network) capacity

Four network limits, none of which a bond's size moves:

### 6.1 A model class's room — `c_class`, shared by every bond

`cap_c = ⌊ready_eff × 1.68e12 × window_c / (5 × eccu_c)⌋` less other classes' terms (§3 gate 3).
For 8k on testnet-12 (8 equal genesis seats ready): **c_8k = 5 unlicensed 8k claims network-wide**.
At the measured cycle (accept → counted licence 23 DAA) that is **≈ 5 × 80/23 ≈ 17 claims per 80
DAA for the whole network**, whoever produces them. `c_8k` moves only with ready seats:

| ready_eff | 5 | 6 | 7 | 8 (today) | 9 | 10 | 11 | 12 | 13 |
|---|---|---|---|---|---|---|---|---|---|
| c_8k | 3 | 4 | 5 | 5 | 6 | 7 | 7 | 8 | 9 |
| per-bond share ⌈c/2⌉ | 2 | 2 | 3 | 3 | 3 | 4 | 4 | 4 | 5 |

Measured: two 100k bonds together (`H`) got 3 + 2 per 23-DAA cycle — **35 claims in 160 DAA =
17.5 per 80 DAA, the whole room** — however much collateral they post.

(`ready_eff = min(ready operators, max(5, ⌊ΣW/W_max⌋))`: a 1M-cap operator beside many 130k seats
lowers it — SW-A6. Below 7 ready seats the class is not live at all, `required_ready_seats`.)

### 6.2 Seat capital — the floor's (and every class's) real network limit

Per bound claim each of the 5 seats reserves a duty `min(max(λ, lock_2), (w+E)/5)` = **640.17 MSK**
(floor; 645.11 for 8k) from bind to Final (`state:3122`), then each counted Valid seat keeps a lock from
Final to Final + `window_court` (3,000 DAA) (`state:3105`, liveness `palw_panel_var_v1.rs:221`),
priced `⌈1.1·g_res/k′⌉ + ⌈0.1·(E − s_cap)/k′⌉` with `k′ = max(basis_k, 2)`:

| licence door | Valid seats locked | lock per seat | locks per claim | measured by |
|---|---|---|---|---|
| **coverage, all five (k = 2)** — what an honest full panel produces, and what releases the escrow | 5 | **240.1 MSK** | **1,200.6 MSK** | this harness: the cards' lock excess = 1,200.6 × Finals at every sample (rel 150–200) |
| V1 quorum, three full-mask (k = 3) | 3 | 160.09 | 480.3 | `rcore/f1-bind-deadlock` m0 |

Both sit under the seat's 500‰ ceiling (469,531.6 MSK per genesis card). Measured in the harness
(`A_floor_small`, 2.48 claims/DAA): duty rows × 640.2 = the cards' duty term exactly (117,151 MSK over
183 rows at rel 200).

```
seat capital per floor claim, coverage = 5 × 640.17 × 122 + 1,200.6 × 3,000 = 3.99 M MSK·DAA
                           V1 quorum  =                       + 480.3 × 3,000 = 1.83 M
                           live mix (89 % coverage)                           = 3.75 M
network long-run floor capacity, 8 genesis seats (shipped): 8 × 469,531.6 / 3.75 M = 1.00 claim/DAA ≈ 80 / 80 DAA
                                   (all coverage 0.94/DAA = 75/80; all V1 quorum 2.05/DAA = 164/80)
post-DAA-500 V02(a): resolved-claim locks leave the 500‰ ceiling, bounded by C − 52,000 accuser reserve:
                     8 × 887,063 / 3.75 M = 1.89 claims/DAA ≈ 151 / 80 DAA   (duties alone would allow 9.6/DAA)
```

The honest all-seat licence — the one that frees the producer's escrow — costs the panel 2.5× the
lock capital of a three-seat licence: capacity for producers and capacity for seats pull in opposite
directions at the door.

The live network already produces ≈ 2.7 floor + 0.12 8k claims/DAA (b1 ≈ 104/80, b6 ≈ 103/80,
b0 ≈ 8/80), far above both numbers. Pure seats' committed grew ≈ 1.1–1.3k MSK/DAA until the first
Finals (DAA ~143) and ≈ 0.4–0.6k MSK/DAA since (b2 153k at DAA 143 → 172k at DAA 177) — the lock
inflow. Projection at the live rate (`seatcap.py`, 2.8 claims/DAA, live door mix): per-seat duties
≈ 137k plus locks growing ≈ 390 MSK/DAA/seat reach the ceiling at **≈ DAA 1,025 on the shipped rule**
(≈ 35 h after DAA 180, i.e. ~Sep 27 evening JST at 150 s/DAA) and ≈ DAA 2,090 with V02(a) armed at
500. When the second seat saturates, SW-10 (eligible stake ≥ 875‰ of the base,
`consensus/core/src/palw_panel_v2.rs:883`, `:1350`) refuses every draw and every claim reaching its
anchor voids `BindTimeout` (no forfeit, escrow burned) — the network is then throttled to the lock
expiry rate (the first locks expire at Final + 3,000 ≈ DAA 3,140), not to any bond's cap.

### 6.3 Seat saturation measured (harness run `K_floor_big`)

Three big floor bonds (250k, 500k, 1M: 39 + 78 + 156 claims at once) plus the background producer
drive ≈ 11 claims/DAA through the eight genesis seats (160 DAA):

| rel (DAA after plant) | accepted / 10 DAA | bound | licensed | voided at the anchor | seat committed (8 cards, k MSK) |
|---|---|---|---|---|---|
| 20–99 | 105–114 | 105–114 | 106–117 | 0 | 72–83 (rel 20) → 406–429 (rel 100) |
| 100–109 | 130 | 89 | 110 | 19 | 430–456 |
| 110–119 | 143 | 15 | 48 | 94 | **442–469.2 = the 500‰ ceiling** |
| 120–129 | 126 | 8 | 8 | 122 | 455–469 |
| 130–139 | 147 | **0** | **0** | **143** | 458–469 |
| 140–149 | 111 | 61 | 47 | 65 | the first Finals release duties (640 → lock 240) |
| 150–159 | 154 | 76 | 82 | 71 | 456–469 |

514 claims voided `BindTimeout` in their own anchor block (accept + 20/21), their escrow burned, while
the producers — each far below its own ceiling once its claims voided — kept producing. The seats'
capital, not any bond, set the network's throughput: ≈ 11/DAA while duties alone fitted, **0** for
ten DAA at saturation, then the rate at which Finals convert 640-MSK duties into 240-MSK locks
(≈ 5–8/DAA here, falling as locks accumulate toward the 1.0/DAA long-run figure). This is the
state the live network reaches at ≈ DAA 1,025 at its current 2.8/DAA (§6.2).

### 6.4 Licence carriage: three five-receipt licences per block

A coverage licence (five V3 receipts) rides a 0x4b carrier of **125,768 transient mass** (payload
23,951 B × 4 + the carrier's own signature and key), and a block's transient mass limit is 500,000
(`consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:149`): **at most 3 per block**
(measured: the fourth is refused, "block transient storage mass 503,072 exceeds limit of 500,000").
A three-receipt V1 licence (~84k) fits five. Network licence throughput is therefore ≤ 3 × blocks per
DAA; with the live ≈ 3–4 blocks per DAA that is ≈ 9–12 licences/DAA — above today's demand, but the
harness saw it add 1–3 DAA of latency whenever ≥ 4 licences fell due together (the 100k floor
batch of 15 licensed over rel 21–23).

### 6.5 Anchors

A claim binds only in an attempt block at or past its slot. On today's live chain b1/b6 publish
≈ 2.6 attempts/DAA, so the wait is < 1 DAA. The at-ceiling producer cannot anchor its own claims
(admission item 8 disqualifies its block — the bind deadlock of `rcore/f1-bind-deadlock`), so a
network whose only producers are at their ceilings binds nothing until the bind window voids the
claims at +600 (§12).

## 7. Model comparison: which parameters decide capacity when a model is added

For an added class `c` with registry work `(vccu_c, eccu_c)` on today's 8 ready seats:

```
window_c   = ⌈2 × vccu_c / 2.4e12⌉ + 1                          (palw_model_registry_v1.rs:173)
c_class    = ⌊8 × 1.68e12 × window_c / (5 × eccu_c)⌋ − other classes' owed replay   (§3 gate 3)
C7 (held to Final, static cap)  iff window_c ≥ 1,000 spans or listed conservative   (state:3157)
reserve_c  = E + ⌊eccu_c × 7,708 / 21,657,728⌋ × 5 sompi × attempts_c          (§2.2)
N_share    = ⌈c_class / 2⌉ ; N_bond = ⌊C/2 / reserve_c⌋ ; throughput ≈ min(N_share, N_bond, room) × 80 / H_c
```

| model | context | reserve / claim | exposure-ceiling claims 13k / 26k / 100k / 1M | c_class | seat count (panel / ready needed) | verification window | licence latency | 13k | 26k | 100k | 80-DAA per bond | limiting factor |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| floor | ~8 tokens (5 MB model) | 3,200.95 | 2 / 4 / 15 / 156 | none (base) | 5 / — | 2 spans | 21 (live p50) | 2 inst, 7.9/80 | 4, 15.9 | 15, 59.6 | 7.9 / 15.9 / 59.6 | collateral (then seat capital network-wide) |
| Qwen2.5 8k | 8,192 | 3,225.56 | 2 / 4 / 15 / 155 | **5** | 5 (1 full + 4 partial) / 7 | 3 spans | 23 | 2 inst, **1 sustained**, 3.4/80 | 3, T-2(a) | 3, T-2(a) | 3.4 / 10.2 / 10.2 (measured) | 13k: collateral (the weight hold); ≥ 21k: T-2(a) and the class room |
| Qwen2.5 2M (dense A16, the 8k row's weights) | 2,097,152 | 62,943.79 | 0 / 0 / 0 / 7 | 1 (C7) | 5 / 7 | 2,799 spans | ≈ the 2,799-span replay (~9.7 d) | 0 | 0 | 0 | 0 (closed); ≤ 1 per ~2,900 DAA network-wide when open | lifecycle + NM + C7 cap 1 + replay |
| an added class X (8k-like cost) | — | ≈ 3,200.85 + eccu·1.78e-11 MSK | ≈ the 8k row | ⌊6.72e12 × window / eccu⌋ − others | 5 / 7 | from vccu | ≈ bind + replay | 2 | min(3, share) | share | share × 80/H | c_class (replay budget) |

**What decides capacity when a model is added:** `economic_ccu_per_claim` (it sets c_class, hence the
per-bond share, AND the weight term of the reserve); `verification_ccu` (the window — and ≥ 1,000
spans makes the class C7: cap 1, held to Final); the ready seats that hold the class (`ready_eff`);
the licence latency `H` (anchor delay 20 + replay + the full-service rate); and, for heavy models
only, the reserve's weight term. **What does not:** the model name, `n_ctx` by itself (only through
the two CCUs), `artifact_bytes` (prefetch spans only), the tokenizer and quantization, the
registration bond, the class ticket / lottery target (a production cost), Probation vs Active
(`admission_permille` prices the production share, not the room) — and, for any class whose eccu is
below ~10¹⁴, the reserve itself (E = 3,200.85 is 99 % of it).

## 8. Switch points: where more collateral stops buying capacity

| model | collateral-limited up to | the switch | above it, the limit is |
|---|---|---|---|
| floor, per bond | always (N = ⌊C/2/3,200.954⌋ grows linearly) | — | seat capital, network-wide: one bond alone reaches the shipped long-run capacity (≈ 1.0/DAA) at N ≥ 21 (**C ≳ 135k MSK**) at the harness cycle; with today's other producers every additional floor claim is already above it long-run |
| 8k, instant | 13k–19.3k (N_bond = 2) | **N_bond ≥ 3 at C ≥ 19,354 MSK** (3 × 3,225.56 × 2) | T-2(a) ⌈5/2⌉ = 3 |
| 8k, sustained | up to 20,243 MSK (the licensed claims' weight, 24.716 × n_ℓ with n_ℓ peaking at 18, must fit beside 3 × 3,225.56) | **20,244 MSK** (measured: 20k → 9.1/80, 21k → 10.2/80) | T-2(a) = 3, then the class room (5 network-wide) |
| 2M | never — closed | (would be 1 claim at ≥ 125,888; C7 cap 1 network-wide) | lifecycle / NM / C7 / replay |

**Measured (8k, harness, 200 DAA each):**

| bond | claims/80 steady | DAA held by T-2(a) / by exposure | max committed / ceiling | idle collateral at the tightest DAA |
|---|---|---|---|---|
| 13,000 | 3.4 | 0 / 201 | 6,451.13 / 6,500 | 48.87 MSK (0.02 claim) |
| 20,000 | 9.1 | 143 / 58 | 9,998.00 / 10,000 | 2.00 MSK |
| 21,000 | **10.2** | **201 / 0** | 10,121.58 / 10,500 | 378 MSK |
| 22,000 | 10.2 | 201 / 0 | 10,121.58 / 11,000 | 878 MSK |
| 26,000 | 10.2 | **201 / 0** | 10,121.58 / 13,000 | 2,878 MSK (0.89 claim) |
| 100,000 | 10.2 | **201 / 0** | 10,121.58 / 50,000 | **39,878 MSK (12.4 claims)** |

So for 8k the bond above which more collateral buys nothing is **20,244 MSK sustained** — ⌈2 × (3 ×
3,225.5625 + 18 × 24.716)⌉, the peak of 18 licensed-not-Final claims a 23-DAA cycle keeps in flight
(the measured max committed at 21k … 100k is exactly 10,121.58 = that sum; 20k measured 9.1/80, 21k
10.2/80) — and **19,354 MSK for the instant**; at 100k,
39,878 MSK of the 50,000 ceiling is never used. For the floor there is no such bond per bond — the
limit moves to the network's seat capital (§6.2).

## 9. Adding models: the effect on an existing model's capacity (cross-model interference)

The class room is one replay budget: class `c`'s capacity is its ready seats' replay per span **less
every other class's owed replay per span** (`PalwPanelRateV1::read`, `state:14252-14267`). The
T-2(a) share is computed with no other class (`others = 0`), so it does not move — the room does.
Measured with the chain's own gates (`instant_sweep`, planted owed claims of other classes by a
filler bond, 8k subject 13k / 100k):

| other classes' owed claims | 8k capacity left (network) | 13k subject gets | 100k subject gets | stop |
|---|---|---|---|---|
| none | 5 | 2 | 3 | exposure / T-2(a) |
| another bond's 8k × 1 | 4 | 2 | 3 | exposure / T-2(a) |
| another bond's 8k × 2 | 3 | 2 | 3 | exposure / room |
| another bond's 8k × 3 | 2 | 2 | **2** | room |
| **2M × 1** (C7: owed to Final; term 6.0e12 of the 13.44e12 per-span budget) | **3** | 2 | 3 | room |
| B = an 8k clone (same cost/window) × 1 | 4 | 2 | 3 | T-2(a) |
| B × 2 | 3 | 2 | 3 | room |
| B × 1 + C (half the 8k cost) × 2 | 3 | 2 | 3 | room |
| BIG (4 × 8k cost, window 7) × 1 | 5 − 0.57 → 5 | 2 | 3 | T-2(a) |

So: **Qwen 8k alone → +B → +B+C does not change a 13k bond's instant capacity (2, exposure-bound)**,
but it takes the room from bigger bonds and from the network: every owed claim of an added class costs
the 8k room `cost_B/window_B ÷ cost_8k/window_8k` 8k-claims (1 for a clone, 0.5 for a half-cost
class, 0.57 for a 4×-cost class with a 7-span window), and **one open 2M claim takes 2 of the 5**.
Seats are not added by adding a model — `per_span` counts the same eight genesis seats — so the
network's 8k throughput falls from ≈ 17 per 80 DAA (5 × 80/23) by the same fraction.

Licence latency is untouched by the room (the anchor delay and replay are per claim), except through
two shared resources: the seats' replay time (one seat replays every class it holds — not modelled
by consensus, measured live only as bind → licence) and the seat capital (§6.2: every class's duties
and locks land on the same eight seats, 645 MSK per 8k seat-duty). Adding classes therefore brings
the floor's seat saturation (§6.2) forward in proportion to their claim rate.

The 2M row cannot be tested with a real claim (closed); its term is read by the gate from the planted
owed claim exactly as it would be from a live one.

## 10. Bond splitting

`palw_operator_id_unique` is armed at genesis on testnet-12 (a second bond under one operator key is
refused, `state:26124-26130`), so a split needs distinct operator keys — free, apart from each piece's
13,000 MSK producer floor. The fork-choice lane's `bond_split_probe`
(`rcore/f1-forkchoice-attacks:consensus/core/tests/bond_split_probe.rs`) established that the stake
DRAW is split-neutral below the 1,000,000 MSK weight cap and amplifies only above it (a whale split
into cap-sized operators weighs the sum, up to one seat each). For claim capacity:

| whole | split | floor N (whole → split) | 8k N (whole → split) | measured (instant_sweep) |
|---|---|---|---|---|
| 26,000 | 2 × 13,000 | 4 → 2 + 2 = 4 (neutral) | **3 → 2 + 2 = 4** | pipeline `G`: 2 + 2 at DAA 0, then 1 + 1 every 23 DAA: **16 in 160 DAA** vs one 26k bond's 21 (`E`) |
| 39,000 | 3 × 13,000 | 6 → 6 | 3 → 2 + 2 + 1 = **5** (room) | — |
| 100,000 | 7 × 13,000 (+9k) | 15 → 14 | 3 → **5** (room) | instant: 100k + 250k → 3 then 2 (room 5); pipeline `H` (100k + 100k): 3 + 2 per 23-DAA cycle, **35 in 160 DAA = 17.5/80 = the whole 8k room**, split 20 / 15 by who asks first |
| 1,000,000 | 76 × 13,000 | 156 → 152 | 3 → 5 (room) | — |

* **Floor:** splitting is neutral to slightly negative (floors of ⌊C/2/3,200.954⌋ per piece). N_bond is
  linear in C, so no gain.
* **8k:** T-2(a) is per bond, so splitting beats it: 26,000 MSK buys 3 claims as one bond and 4 as two
  13k bonds (+33 %), and any operator with ≥ 3 bonds can take the class's whole room (5) where one
  bond gets 3. The room (5 network-wide) is the only thing that stops it — T-2(a) is capital pricing,
  not identity (ADR-0152 v3.1 wording), and the measurement confirms it gives no protection against a
  split. But each 13k piece sustains only 1 claim (§4.2), so the steady-state gain is smaller:
  2 × 13k sustain ≈ 2 × 3.4 = 6.8/80 vs one 26k bond 10.2/80 (measured) — **for sustained 8k throughput one
  26k bond beats two 13k bonds**; the split wins only the instant.

## 11. Theory vs measurement — every difference is a finding

| # | expectation (theory / prior estimate) | measured | why |
|---|---|---|---|
| F1 | 8k at 13k: "2 claims" (6,500 / 3,225.56 = 2.01) | **2 at DAA 0, then 1 sustained**: 3.4/80, not 2 × 80/23 = 7.0 (harness `C_8k_13k`) | the licensed claims' weight (24.716 MSK each) stays to Final; two of them exceed the 48.875 MSK slack above two full reservations (§4.2) |
| F2 | 8k at 20k: 3 (T-2(a)) | 3 until 15 claims are licensed-not-Final, then 2 ↔ 3: **9.1/80** vs 10.2 at 26k/100k (`D_8k_20k`: committed 9,973.28 at rel 92 → exposure binds from rel 115) | same weight hold: 3 × 3,225.56 + n_ℓ × 24.716 ≤ 10,000 caps n_ℓ at 13 < the 16–18 a 23-DAA cycle leaves in flight |
| F3 | live b0 (939k) holds 3 concurrent 8k claims (T-2(a)) | **1 usable slot from DAA ~160**: `notReadyReason` "holds 3 unlicensed claims" with one Provisional | two S2 licences (claims accepted 114 and 139, one Valid each: the full seat) count as unlicensed (`state:3148`) and are not due before bound + 601 |
| F4 | floor escrow leaves at the licence | only for 88.9 % of live licences (281/316 had all five Valid); 6.6 % keep it to Final, 4.4 % (S2) to ≥ bound + 601; **no licence gained a receipt later** in 40 min of snapshots | SR-1 needs every seat served (`state:3185-3205`); the V2/V3 supplementary doors exist but no collector used them on live |
| F5 | a block carries as many licences as fees allow | **3 five-receipt licences per block** (125,768 transient mass each; the 4th refused at 503,072 > 500,000) | `body_validation_in_isolation.rs:149`; licence latency in the harness grew 21 → 23 when ≥ 4 fell due together |
| F6 | the registry row's `maxInflightClaims 5` is the 8k cap; `inflightClaims 13` exceeds it | neither is the gate: past the work target the room is the RATE rule (c_8k = 5 by `palw_panel_capacity_by_rate_v1`, coincidentally also 5) and it counts only unlicensed claims (`utilizationPermille 516` = 3 owed of 5.8), while `inflightClaims` counts licensed ones too | an RPC reading trap, not a violation |
| F7 | judge (int-2): w_8k = 494.32 MSK held L→F ⇒ 13k 8k ≈ 0.044/DAA | shipped w_8k = **24.716** (the derived basis, ADR-0145/0149); the hold still binds at 13k (F1) but costs 1 slot, not the bond | the judge used the pre-derived unit |
| F8 | judge (v3.1): 13k floor ≈ 6.2/80 | harness (ideal seats) **7.9/80**; live-like model **7.1 in the first 80, 3.1/80 steady** | the ideal is H = 21; live latency (mean 23.9) and the 11 % non-full licences cut it by half at steady state |
| F9 | the task statement's N_share = ⌈c_class/2⌉ | confirmed: c_8k = 5 → 3 for every bond ≥ 19,354 MSK; the room (5) is shared, so a SECOND 8k bond gets at most 2 while the first holds 3 | `instant_sweep` split rows |
| F10 | model (capmodel2/3) vs harness | floor 13k 7.6 vs 7.9; 8k 13k 3.5 vs 3.4; 8k 20k 8.8 vs 9.1; 8k 26k+ 10.5 vs 10.2 per 80 DAA | within one claim per 80 DAA; the model's licence day is a DAA earlier than the harness's carriage |
| F11 | N_instant by formula | **equal to the chain's gates on every cell** of §3.1 (24 cells) | — |
| F12 | first sweep read the 8k network room as 4 | 5 — the sweep's own template 8k claim (card 0) was in flight | confirms F9: any bond's in-flight 8k claim takes room from every other bond |
| F13 | seat lock per Valid seat = 160.09 MSK (`rcore/f1-bind-deadlock` m0), so ≈ 1.35 claims/DAA network capacity | **240.1 MSK per seat at the coverage door** (k = 2, five seats: 1,200.6 per claim), 160.09 only at the three-seat V1 quorum (k = 3); network long-run capacity ≈ 1.0/DAA at the live door mix, saturation ≈ DAA 1,025 not ≈ 1,400 | the lock's `k′ = max(basis_k, 2)`: coverage recounts to 2 |
| F14 | an S2 (optimistic, k = 1) licence is a licence | measured: S2 licences keep the T-2(a) slot AND the escrow; a 100k bond with three S2 licences produced **0 claims** after its first three, and the three were still `ReceiptLicensed`, unreleased and counted at DAA 234 — past L + 121 = 178 (`S2_8k_100k_200`), and with the live mix a 100k bond froze from rel ~160 (`L2`), a 13k bond from rel ~40 (`L3`) | `palw_rcore_counts_licensed_v1` needs k ≥ 2; SR-1 needs Quorum/Coverage and every seat; DL-1 dates an unupgraded S2 at `max(L + 120, bound + 601)` (`state:3570-3585`) and the live collectors never sent the supplementary sets that would upgrade it |

## 12. What changes after the DAA-500 fence release (post-DAA-500)

Read from the lanes (not edited): `rcore/f1-v02-lock-budget`, `rcore/f1-seat-maturity[-ext]`,
`rcore/f1-bind-deadlock` (now with its fix, `palw_anchor_at_ceiling`), `rcore/f1-panel-seed` (lane A,
`palw_operator_anchor`), `rcore/f1-v04-licence` and `rcore/n1-carrier` (node policy, in `rcore/int-4`).

| row | shipped | post-DAA-500 | change |
|---|---|---|---|
| §3 per-bond gates (N_bond, T-2(a), room) | as §3 | unchanged — no lane touches `apply_attempt`'s exposure term, the share or the rate room | 0 |
| §4 per-bond claims/80 (ideal seats) | floor 13k 7.9, 8k 13k 3.4, 8k ≥ 26k 10.2 | unchanged | 0 |
| §5 licence latency tail (live floor p90 30 / p99 59 / max 86) | claim-id-ordered offering | V04: oldest-bind-first, one carrier per duty (V01), readiness proofs on their own output (V07) — the pre-t12 drill measured the id-ordered wait at 6.0 DAA mean / 28 max for high ids vs 0.8 / 8 for low ids | tail → ≈ the low-id figures; median unchanged (21 / 23); alone it moves floor 13k live-like only 7.1 → 7.5 in the first 80 DAA and not at steady state (model: bind → licence capped at 8) |
| §5.3 non-full licences (11 % floor, 31 % 8k) | escrow held to Final / S2 to bound + 601 | V04/V07 remove the node-side causes of partial panels (starved carriers, readiness proofs blocking receipts); no consensus change to SR-1 | if they bring full service to ≈ 97 % (mix 97/2/1), live-like steady rises: floor 13k 3.1 → 5.1–5.3/80, 26k 5.9 → 10.1–10.4, 100k 21.7 → 38.6–40.0; 8k 13k 0.9 → 2.9, 8k ≥ 26k 1.7–1.9 → 7.8–8.4 (model); the ideal (all five, every time) is 6.9 / 13.8 / 51.3 and 3.4 / 10.0 |
| §6.2 seat capital (network floor capacity) | ≈ 1.0 claim/DAA (80/80, live door mix); saturation at the live 2.8/DAA ≈ DAA 1,025 | V02(a) `palw_final_lock_full_collateral`: resolved-claim locks count against C − 52,000 (accuser reserve), not the 500‰ ceiling | **≈ 1.89 claims/DAA (151/80), ×1.89**; saturation at the live rate ≈ DAA 2,090 instead of ≈ 1,025 |
| producer-seats (b1/b6/b0) own capacity | own claims + duties + locks under one 500‰ | V02 moves their resolved locks off the ceiling | b1/b6 keep producing as locks pile up (their own ceiling would otherwise fill first) |
| §6.1 c_8k growth by new seats | a new 130k seat is drawable at once | seat maturity: drawable, counted ready and on the admission jury only from registered + 1,000 DAA | adding seats raises c_8k (§6.1 table) only after ≈ 1,000 DAA (≈ 42 h) |
| §6.5 anchors / the bind deadlock | the first attempt block at or past the slot anchors; an at-ceiling producer's attempt is disqualified — a network of at-ceiling producers binds nothing for 600 DAA | lane A: only the 8 genesis operators' attempts anchor (a 13k producer's own attempts never bind its claims); bind-deadlock: an at-ceiling OPERATOR attempt is a binder | licence latency for a 13k bond = slot + wait for the next operator attempt (live b1+b6 ≈ 2.6/DAA → +0.4 DAA mean); risk moves to "operators stop publishing" (580 DAA of slack, then S0 void) |
| 2M | closed | still closed (U-D1: opens only at a flag day with measured rows) | 0 |

## 13. 2M collateral decomposition and the 1/10 – 1/100 study

### 13.1 The one formula, fully expanded (0e8ec984e; harness `collateral_study` + live facts)

| term | floor | 8k | **2M** | code |
|---|---|---|---|---|
| declared `pwu_per_inference` (class rule) | 7,708 | 105,518,224 | 27,002,967,184 | `ClassRegistered.pwu_rule`, `state:4046` |
| registry `economic_ccu_per_claim` (the derived draw) | 21,657,728 | 1,388,926,972,416 | 3,357,281,757,221,376 | `state:9564-9570` |
| **derived exposure PWU** = ⌊eccu × 7,708 / 21,657,728⌋ | 7,708 | 494,320,046 | **1,194,858,841,364** | `state:4002-4024`, basis `:9629-9642` |
| × `slash_value_per_pwu` | 5 sompi | 5 | **5** | `palw_fp_devnet_v3.rs:547`; one per network (`SlashValueNotTheNetworks`) |
| × attempts (claim_pwu / eccu) | 279 | 1 | 1 | `palw_pwu.rs:179-184` |
| **PWU × slash value = w** | 0.1075 MSK | 24.716 MSK | **59,742.94 MSK** | `state:30979-30983` |
| **escrow E** = ⌊4,445.62014 × 720‰⌋ | 3,200.8465 | 3,200.8465 | **3,200.8465** | `state:29444`, `params.rs:16579` |
| weight reservation | = w (the `reserved` field IS the weight term) | | | `state:4608` |
| **total reserved / claim** | 3,200.9540 | 3,225.5625 | **62,943.7886** | |
| exposure ceiling | 500‰ | 500‰ | 500‰ | `fp_max_exposure_ratio_permille` |
| **minimum collateral for one claim** = reserve / 0.5 | 6,401.9 (→ 13,000 floor) | 6,451.1 (→ 13,000) | **125,887.58** | |

**Yes: 125,888 = 2 × 62,944**, the same structure as the floor's 3,200.95 → 6,401.9. Of the 62,944:
**59,742.94 (94.9 %) is PWU × slash_value, 3,200.85 (5.1 %) is escrow**, nothing else. The escrow is
the carrying block's worker carve (the miner reward, withheld to licence/Final); it is identical for
every class, so only for the floor and 8k — where it is 99.997 % / 99.2 % of the reserve — is the bond
exposure reward-derived. For 2M it is compute-derived, exactly as the user's reading has it.

Two couplings the code adds beyond "PWU × slash value":

* the **derived** basis (ADR-0145/0149) already makes 2M's exposure PWU 1,194,858,841,364 instead of the
  declared 27,002,967,184 (which would have been 135,015 MSK);
* the **work target** (ADR-0137, armed from genesis) prices any class cheaper than W₀ = escrow/rate ≈
  306 G MAC-eq at `W₀/CCU` attempts, so cutting a light class's PWU re-inflates `attempts` (measured:
  8k eccu ÷10 → attempts 2, ÷100 → attempts 25). 2M is 11,000× above W₀, so it scales linearly.

### 13.2 The 1/10 and 1/100 cuts — measured on planted variants (the fold's own facts)

"PWU ÷ f" divides the class row's work (eccu and vccu; the registry profile re-derived); "svp = 1" is
the smallest non-zero cut an integer sompi allows (÷5) — **÷10 and ÷100 are not representable** with
`slash_value_per_pwu: u64 = 5`; their rows below are the arithmetic, identical in exposure to the
PWU rows for 2M.

| variant | class | reserve (MSK) | N at 13k / 26k / 100k / 1M | T-2(a) / C7 | measured |
|---|---|---|---|---|---|
| shipped | 2M | 62,943.79 | 0 / 0 / 0 / 7 | C7 cap 1 | yes |
| PWU ÷ 10 | 2M | 9,175.14 | 0 / 1 / 5 / 54 | C7 (conservative list), window 2,799 → 281 | yes |
| PWU ÷ 100 | 2M | 3,798.28 | 1 / 3 / 13 / 131 | C7 (list), window → 29 | yes |
| svp 5 → 1 (÷5) | 2M | 15,149.43 | 0 / 0 / 3 / 33 | C7 | yes |
| svp ÷ 10 / ÷ 100 | 2M | 9,175.14 / 3,798.28 | as PWU ÷ 10 / ÷ 100 | C7 | arithmetic |
| any cut | 2M, floor of E | ≥ 3,200.85 | ≤ 2 / 4 / 15 / 156 | | the ceiling: at most **19.7×** more claims per bond, never 100× |
| PWU ÷ 10 / ÷ 100 | 8k | 3,205.79 / 3,207.03 | 2 / 4 / 15 / 155 (unchanged) | share 3 → **19 / 194** (c_class 38 / 389 — the room now believes replay is 10–100× cheaper) | yes |
| svp 5 → 1 | 8k | 3,205.79 | 2 / 4 / 15 / 155 | share 3 | yes |
| any | floor | 3,200.87–3,200.95 | 2 / 4 / 15 / 156 | — | yes |

**Throughput (claims/80 DAA), which is what the cut was meant to raise:**

* **2M: unchanged at 0.** It is closed at launch (Prefetching; NM `ClassDeadlineUnmeasured`: 5 ×
  2,799 = 13,995 > receipt window 600; cap-utilization 18,879,717‰ > 800‰, not activatable). Opened,
  C7 holds it to **one 2M claim network-wide until Final** and one replay is ~2,799 reference spans
  (~9.7 days): ≤ 1 claim per ~2,900 DAA whatever the bond. A cheaper reserve only lowers the minimum
  bond for that one claim (125,888 → 18,350 at ÷10 → 13,000 floor at ÷100).
* **8k: at most ×2 at 13k, nothing above.** The cut removes the weight hold of §4.2 (w 24.7 → ≤ 6.2 MSK:
  n_ℓ fits beside two unlicensed), so 13k sustains 2 (≈ 6.9/80 instead of 3.4); from 21k up T-2(a) and
  the room bind as before. (PWU ÷ 10 also lifts T-2(a) to 19 — by lying to the room about replay cost.)
* **floor: nothing** — E is the reserve.

### 13.3 Safety against the design bar (`claim-collateral-design-bar`, `bond-throughput-judge`)

* **Cash a fraudulent claim can capture ≤ E for every class**: a model-class Final is paid
  `min(escrow, attempted × rate)` (ADR-0132 C, `params.rs:16387-16394`; 2M's uncapped reward is
  18,880× E), and E vests in a burnable row. So the bar "max extractable ≤ recoverable" is already met
  by E alone for cash. **w is not a cash cover — it prices fork-choice WEIGHT**: `G = w + E` in
  ADR-0152's accounting, and a 2M claim carries claim_pwu 3.357e15 (immature β·pwu = 3.36e14) against
  the floor's 6.04e9 — one 2M claim weighs ≈ 555,600 floor claims. svp is "per network because it sets
  weight per collateral" (`params.rs:12049`): today every class pays 5 sompi of w per exposure-PWU,
  i.e. the same weight per MSK of w; cutting svp 10–100× makes a private fork's weight 10–100× cheaper
  per MSK at risk for EVERY class. The design bar's own condition applies: cutting the weight term is
  safe "only if the fork-choice harm of many fraudulent claims is capped (per miner / panel / competing
  state) rather than additive" — J-1 (per-bond weight cap) is not shipped.
* `P*·H_L ≤ C / (r·G)`: G = 62,944 (2M) → 9,175 at ÷10; with C7's one claim network-wide, the
  per-bond P* line is moot for 2M; for 8k G changes by < 1 %.
* **Max uncovered loss at 1,000 claims:** unchanged in count for 2M (C7: 1 live claim network-wide).
  Per claim, the (C) acceptance already records ≤ 63,599 MSK uncovered for an unattributable 2M claim
  (it cannot be convicted, so its whole reserve is the exposure). A svp cut shrinks that reserve to
  9,175 / 3,798 MSK: the uncovered CASH stays ≤ E, but the fork-choice weight one such claim buys
  per MSK at risk grows 6.9× / 16.6× — the weight harm becomes the uncovered part.
* **What PWU feeds that svp does not** (why a PWU cut has side effects): the claim's fork weight
  (`claim.pwu`, `immature_contribution_v2` β = 100‰, `state:2253`); the room / c_class (eccu = replay
  cost — a cut makes the panel accept 10–100× more replay than it can do, and receipts then miss the
  600-DAA window); the work target's attempts (W₀/CCU); execution-lane credit (`exposure_pwu`,
  `palw_execution_lane_v1.rs:282`); the payout's `attempted × rate` and cap-utilization (activatability);
  the registry window (C7 membership, NM gate) and the registration bond. **What svp feeds:** w (the
  reserve and what a void/conviction takes, `claim.reserved`), the seats' duty `(w+E)/5` and lock
  (`g_res` contains w: a 2M seat duty is ≈ 12,589 MSK today, (w + E)/5, and the readiness predicate
  asks free collateral of 3× the seat exposure), and weight-per-collateral — for every class at once.

### 13.4 Verdicts

| option | 2M | 8k | floor | verdict |
|---|---|---|---|---|
| PWU × 1/10 | reserve 9,175 (×6.9 bond-claims), throughput unchanged (closed / C7 / replay) | ≤ 1 %, T-2(a) inflated by a false replay cost | none | **not realistic** — it misstates compute to the room, the weight, the work target and the lane credit |
| PWU × 1/100 | 3,798 (×16.6), throughput unchanged | same, worse | none | **not realistic** |
| svp × 1/10 | 9,175, throughput unchanged | 13k sustained 1 → 2 (≈ ×2) | none | **realistic only with conditions**: a finer svp unit (5 sompi cannot be divided by 10); J-1 per-bond fork-choice weight cap shipped first; 2M stays closed or becomes attributable (F1–F4 / A-held for 2M); a fence + re-pin + fence-crossing drill |
| svp × 1/100 | 3,798, throughput unchanged | as ÷10 | none | same conditions; weight becomes ~100× cheaper per MSK — not before J-1 |

**Minimum-risk knob for ~10× more claims:** none of the two. For 2M the limit is C7 + replay, not
collateral (at most 19.7× more bond-claims even at w = 0, and 0 more throughput). For floor/8k the
reserve is the withheld reward E; the levers are the ones in `bond-throughput-judge`: a slot reserve
u ≈ E/4 + w with aggregate forfeiture (J-2/J-3), a shorter hold (anchor_delay 20 → 4), releasing the
escrow at a k ≥ 2 licence instead of at full service, and for 8k a larger replay budget (ready seats).

**Consensus:** both are consensus parameters — svp lives in each class's rooted state
(`classes[id].slash_value_per_pwu`, set by the genesis `ClassRegistered` objects) and eccu in the
registry's derived work (genesis works → rows); changing either on the live t12 is a fenced rule change
(fork id / params id / re-pin / fence-crossing drill), a candidate for a later fence, not implemented.

## 14. How it was measured, and how to reproduce it

**Harness** — `consensus/src/pipeline/virtual_processor/tests/t12_claim_capacity.rs` on
`audit/claim-capacity` (base `0e8ec984e`). testnet-12 as shipped with harness keys on the eight genesis
cards (`t12_round_lane_e2e::t12_with_harness_cards`), heartbeats to DAA 34 (past the registry grace),
then planted subject bonds (pure producers, keys 8 + j, no capable classes), and for 8k planted
readiness on the eight cards (refreshed every 16 DAA). Every block after the plant is the node's own:
template, heartbeat adapter, attempt envelope (real ML-DSA-87), the chain's panel derivation and stake
draw, V3 duty receipts signed by the drawn seats, carriers taken by the block's transient-mass budget.
The all-seat licence object is asserted equal to the node's coverage assembler's once per run and then
built directly (the assembler clones the tip per candidate).

```
export CARGO_TARGET_DIR=… CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
nice -n 15 cargo test -p kaspa-consensus --lib --no-run
cd consensus
CAP_RUN=C_8k_13k CAP_CLASS=8k CAP_BONDS=13000 CAP_POLICY=all5 CAP_LIC_DELAY=2 CAP_DAA=200 CAP_OUT=/tmp/c.jsonl \
  <test binary> --exact pipeline::virtual_processor::tests::t12_claim_capacity::t12_capacity_run --ignored --nocapture
<test binary> --exact …::t12_capacity_instant_sweep --ignored --nocapture       # §3.1, §9, §10
<test binary> --exact …::t12_capacity_collateral_study --ignored --nocapture    # §2.2, §13
```

`CAP_POLICY`: `all5` | `quorum3` | `s2` | `mix:<all5‰>,<quorum3‰>,<s2‰>`; `CAP_LIC_DELAY`: DAA after the
bind, or `live:floor` / `live:8k` (the live bind → licence histograms). Each run takes 1.5–8 min of a
debug build on a loaded machine.

| run | class | bonds (MSK) | policy / licence | DAA |
|---|---|---|---|---|
| A_floor_small | floor | 13k, 20k, 26k, 50k, 100k (one chain) | all5, bind + 1 | 200 |
| C / D / E / F | 8k | 13k / 20k / 26k / 100k | all5, bind + 3 | 200 |
| G / H | 8k | 13k + 13k / 100k + 100k | all5, bind + 3 | 160 |
| I / J | 8k / floor | 13k | quorum3 (escrow held to Final) | 200 |
| K_floor_big | floor | 250k, 500k, 1M | all5 | 160 |
| L1 / L2 / L3 | floor / 8k / 8k | 13k + 26k / 100k / 13k | live mix, live latency | 300 |
| S2_8k_100k / S2_8k_100k_200 | 8k | 100k | s2 | 120 / 200 |
| M / N | 8k | 21k / 22k | all5, bind + 3 | 200 |

**Model** — `capmodel.py`, `capmodel2.py`, `capmodel3.py`, `seatcap.py` (the same gates as §3; used
for 1000-DAA horizons, live-like latency/mix, and network seat capital).

**Live** — read-only wRPC to `wss://misakascan.com/kaspa` every 5 min from DAA 143:
`getBlockDagInfo`, `getPalwModelRegistry`, `getPalwProducerFacts` (floor and 8k, all eight genesis
bonds, including `notReadyReason`), `getPalwClaims` (executor and seat roles), `getPalwPanelAssignments`
(Valid receipt count per claim). No drill was run; no host was touched.

Measurement data, compacted, is committed beside this file in `docs/palw-claim-capacity-map-2026-09-26/`:
`runs/<run>.json` (per subject per DAA: `[rel, daa, accepted_now, accepted_cum, unlicensed, escrow_held_licensed,
committed MSK, hold, phases]`; per 5 DAA the eight cards' `[i, committed, ceiling, own, own_held, duties,
duty_rows]`; every claim's `[subject, accepted, bound, licensed, released, final, void]`; the run's
parameter and sweep lines), `live/` (the live latency / licence-mix histograms and the snapshot
series), and the model scripts (`model/`).

## 15. Distance to the standing goal: ~100 claims per 80 DAA from one 13,000 MSK bond

| | floor | 8k |
|---|---|---|
| shipped, ideal seats (harness) | 7.9 / 80 (2 slots × 80/20.2) | 3.4 / 80 (1 sustained slot × 80/23) |
| shipped, live-like (model) | 7.1 first 80, 3.1 steady | 3.6 first 80, 0.9 steady |
| gap to 100 | **×13** (ideal) … ×32 (live-like steady) | **×29** … ×110 |
| what binds | the reserve (E = 3,200.85 withheld reward per claim, half the bond usable) × the hold (licence ≈ 21–24 DAA, Final 142 when a seat did not serve, ≥ 600 DAA for S2) | the same, plus the weight hold (1 slot), plus T-2(a) 3 / room 5 above 20k |
| and the network | seat capital ≈ 80 claims / 80 DAA for ALL floor producers long-run (≈ 151 with V02) — 100/80 from one 13k bond exceeds the whole network | c_8k = 5 unlicensed network-wide ≈ 17 / 80 for ALL 8k producers — 100/80 is 6× the network |

The judge's recommended J-1…J-6 (slot reserve u ≈ 800 with aggregate forfeiture, K = ⌊2C/G⌋ = 8 at
13k, anchor_delay 4, licence release not tied to full service, a floor licence room) is still the
path; this map adds three constraints it did not price: the 3-licences-per-block carriage (§6.4),
the S2 hold (§5.3) and the 8k weight hold (§4.2).
