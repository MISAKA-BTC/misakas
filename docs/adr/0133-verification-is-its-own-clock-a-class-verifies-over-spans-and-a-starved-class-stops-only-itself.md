# ADR-0133 — Verification is its own clock: a class verifies over spans, and a starved class stops only itself

* Status: **PROPOSED 2026-09-17; the profile, the capacity arithmetic, the simulation and the observability
  built in shadow the same day** on `feat/palw-exec-lane-and-validator-retirement`. No consensus rule,
  parameter or fingerprint moves; nothing activates on testnet-11.
* Operator's direction, in the operator's words: "120秒を「全modelがverificationを完了しなければならない
  deadline」とは扱わない"; "Execution cadence / PALW anchor cadence / class verification deadline を別々の時間軸
  として設計"; "大型modelのliveness不足がPALW全体やExecution Lane全体を停止させないこと"; "verification window を
  producer 自身が選択できないこと"; "Compute Credit は Final 後にのみ発生"; "artifact 転送が遅いことを Economic CCU
  へ単純に加算しないこと"; "BPS1 と PALW Anchor 120 秒は分析なしに変更しない"; "beacon を復活させない".
* Builds on: [0132](0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md)
  (the end-to-end read), [0131](0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md)
  (economic compute), [0130](0130-bps1-is-hardened-before-it-is-widened.md) (spans, the shadow-first discipline),
  [0125](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)
  (credits from `Final`s), [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  (seat exposure), [0112](0112-the-weight-residency-budget.md) (residency), [0098](0098-a-panels-coverage-of-one-claim-is-a-number.md)
  (what sampling catches), [0071](0071-hash-coupling-and-the-remainder.md) SA-3 (capability by production).

## 0. The sentence this ADR is

**No claim on this chain has ever had to verify inside 120 seconds — a panel holds its claim for six hundred
anchors — so what bounds a large model is not the anchor but its panel's capacity over its window, the bytes a
seat must move before it can replay, and the collateral its duties hold; a class therefore carries a
verification profile derived from its graph and its calibration, never chosen by its producer, and a class
that outruns that profile holds its own new claims while every other class, the execution lane and the anchor
cadence continue.**

## 1. The lifecycle, with every clock it depends on (read without changing code)

| stage | rule | clock |
|---|---|---|
| claim creation | one forward, two lotteries (class ticket ≤ target; Layer-0 digest ≤ `bits`); no search | the producer's own; `bits` retargets to the 120 s cadence |
| accepted | the attempt block is a chain block; `claim.accepted_daa`; escrow withheld; the producer's reservation `pwu_per_inference × slash_value_per_pwu` held | one anchor |
| panel selection | the anchor slot is the first chain block at or past `accepted + anchor_delay (20 DAA)`; the draw is `H(anchor ‖ claim ‖ operator)` over `Active` bonds that declared the class (`palw_capability_bound` `None`: declaration, not proof), the executor's operator excluded | +20 anchors |
| panel_bound | `PalwClaimPhaseV2::PanelBound { bound_daa }`; duty rows and seat exposure `3 × claim.reserved` (or λ × seat reward past ADR-0130) reserved; **`receipt_deadline = bound + window_receipt (600 DAA)`** | 600 anchors |
| artifact availability | a seat resolves the class's backend from the artifacts it loaded at start (`--palw-class-artifact`); none → `Incapable`, which counts toward nothing and shortens nothing | none (node-local) |
| artifact fetch | there is no fetch: weights are placed out of band; residency is a memory budget over a memory map (ADR-0112: a fifth of the weights within what the host has; below the class's floor, the page cache) | none |
| verification start | the seat draws four intervals, asks the executor for openings, waits `PALW_ATTEMPT_REPLAY_GRACE_DAA = 2` anchors, then replays the whole job (on testnet-11 every licence was a replay: 504 replays, 0 openings verified) | +2 anchors, then the seat's own time |
| verification complete | one replay per duty, one duty at a time per seat (`offload` awaits each) | seat time: 20 s (dense, ibm), ~60 s (hybrid, warm), minutes cold |
| receipt submit | the seat signs `Valid / Unavailable / Incapable`; a funded submitter carries receipts in a transaction (`MAX_INFLIGHT_CARRIERS = 8`) | one to two anchors |
| quorum | three same-verdict receipts of five → `ReceiptLicensed { licensed_daa }`; a panel that concludes nothing is redrawn ONCE at the timeout, the second panel gets its own 600 | up to 2 × 620 anchors |
| challenge | `licensed + window_challenge (1,200 DAA)` with no court → `Final` | 1,200 anchors |
| court | `window_court 3,000 DAA` for a bisection (51 moves) | 3,000 anchors |
| Final | escrow priced and split; seat duties released; `record_round_final` credits the lane | +1 block for the coinbase |
| exposure release | producer reservation and seat exposure released at `Final` or void (`ReceiptTimeout` ≈ `accepted + 1,242`) | — |
| reward release | the payout queue, drained per block | anchors |
| compute credit creation | **only** in `finalize_claim` → `record_round_final`; nothing below `Final` credits | — |
| execution permit eligibility | the finals of span `n − 2` are span `n`'s participants, seeded at `n`'s first block by `n − 1`'s last attempt-carrying block (ADR-0130); spans are 5 anchors | spans |

**What breaks past 120 seconds of verification: nothing.** The claim's clocks are 600, 1,200 and 3,000
anchors. What 120 s IS: the cadence of the anchors every window is counted in, the granularity of the bind
(one slot per chain block), the carrier's confirmation unit, and the execution lane's span unit (5 of them).
The eight times the operator asked to keep apart, as testnet-11 shows them:

| | measured | where it is spent |
|---|---|---|
| 1. inference compute | dense 1.5 s a forward (A16); hybrid ~60 s warm | producer; ×7.2 / ×4.9 forwards a claim (both lotteries) |
| 2. panel verification compute | dense replay 20.1 s mean (12–61 s), 497 replays on ibm; hybrid never replayed by any seat | 5 seats × 1 replay a claim |
| 3. artifact transfer | none in-protocol; a 24 GB file copied by the operator | out of band |
| 4. artifact cache miss | hybrid 9.4–9.9 GiB paged in per draw on a 23 GB host (no residency: "the page cache decides"); the same on a seat | seat, per replay, until resident |
| 5. receipt propagation | gossip + a carrier transaction; two anchors allowed | anchors |
| 6. quorum formation | bound → licensed median unobservable (the state drops `bound_daa` at licence); accepted → licence ≈ 20 + the slowest of three seats | anchors |
| 7. challenge / court | 1,200 / 3,000 anchors, the same for every class | anchors |
| 8. collateral lifetime | producer: accepted → `Final` (median 2,017 anchors) or void (1,242); seat: bound → `Final`/void | anchors |

Only 1, 2 and 4 are the model's; 5–8 are the chain's and do not grow with the model. A Kimi-class claim
therefore needs a **verification window** long enough for 2 + 4 + 5 + 6, a **panel** with enough seat-time
for 2 over that window, **residency** that turns 4 into ~0, and **collateral** for the duties in flight — four
separate levers, none of them the anchor cadence.

## 2. The hybrid's measured path and a Kimi-class path

**Qwen3.6 on testnet-11 (ADR-0132 §1):** accepted → bound +20 → no seat able to replay (the only artifact host
is the producer, excluded; a second seat stuck in IBD; the rest hold no artifact) → redraw +641 → void +1,242.
Its producer's draw: 979 forwards, 202 blocks, 9.7 GiB paged in per forward, 2–3 minutes a forward.

**A Kimi-class stand-in** (1 T MAC-eq a draw, a 300 GiB artifact, 900 s a warm replay): at the reference
verifier (4 G MAC-eq/s, the ibm seat) a warm replay is 250 s of arithmetic — the 900 s figure is what memory
traffic costs on a host that cannot hold the working set — and a cold replay adds 322 s at a gigabyte a second
(300 GiB) or 2,700 s at 100 MB/s. Nothing in the chain's clocks refuses it; the panel's capacity does: at the
dense tier's live rate (2.2 claims a span) five seats × 900 s = 9,900 seat-seconds a span against seven seats
× 600 s = 4,200 — 236 %, dead — and at that rate 24 eligible seats sit at 70 %, 26 with two operators to spare.

## 3. Decisions

**Decision 1 — three clocks, kept apart.** The execution round (1 s, ADR-0125), the PALW anchor (120 s, one
chain block; every window of the lattice is denominated in it), and a class's **verification window in whole
execution spans** (5 anchors each). Neither of the first two moves for a model. PWU stays fork choice
(`claim.pwu`), `EconomicAttempted` stays model economics (ADR-0131/0132), the window is liveness/timing, and
artifact availability is data availability/runtime — four responsibilities, four numbers.

**Decision 2 — `PalwVerificationProfileV1`, derived, never chosen.**
`{ verification_window_spans, artifact_prefetch_spans, max_inflight_claims, warm_p99_ms, cold_p99_ms,
seat_count }` from `PalwClassTimingFactsV1 { draw_compute (the graph's, ADR-0131), artifact_bytes (declared at
registration), warm_p99_ms, cold_p99_ms (measured in the shadow period, ADR-0131 Decision 6) }` and a
network reference `PalwVerificationReferenceV1 { mac_eq_per_ms, artifact_bytes_per_ms, receipt_allowance_ms,
safety_permille, utilization_permille, min_eligible_seats }`:

* `warm_p99 = max(measured warm, draw_compute / mac_eq_per_ms)` — under-reporting is floored at the estimate;
* `cold_p99 = max(measured cold, warm_p99 + artifact_bytes / artifact_bytes_per_ms)`;
* `verification_window_spans = ⌈safety × (cold_p99 + receipt_allowance) / span⌉`, one at least — the
  operator's candidate formula, with the **cold** p99 and the receipt allowance inside the bracket;
* `artifact_prefetch_spans = ⌈safety × artifact_bytes / artifact_bytes_per_ms / span⌉` — how far ahead of a
  duty a seat must hold the artifact to verify warm;
* `max_inflight_claims = ⌊utilization × min_eligible_seats × window / (seat_count × warm_p99)⌋` — Little's law
  at the target utilization over the fewest seats the class must have.

Fixed at activation as a consensus value (a fence names the reference; the profile is a function of chain
facts, so two nodes derive one profile), re-derived only by a later fence. **The reference the fleet measured:**
4 G MAC-eq/s, 1 GB/s, two anchors of receipt allowance, ×2, 70 %, seven seats. On it: the dense tier is one
span (2 × (21.6 + 240) s), the hybrid two (a 180 s cold replay), the Kimi stand-in five (2 × (900 + 322 + 240) s).
**Warm and cold apart:** the window is sized on cold (a seat must survive a miss); the prefetch is what makes
the cold case rare; the capacity is sized on the measured cold fraction. Prefetching and sizing the deadline on
warm replays is the right steady state — and the wrong first day, so the window keeps the cold term.

**Decision 3 — class-local fail-closed: the gate.** At the anchor slot, a claim of a class whose bound claims
already number `max_inflight_claims` is **held** — not bound, not refused, not voided until its own bind window
(600 DAA) runs out — and no other class's claim is touched (`palw_class_gate_v1`). The fold site is the sweep
that writes `PanelBound` (`palw_state_v2.rs`, the `"PanelBound"` edge), behind a fence. A class with no
drawable panel (fewer eligible seats than the panel) voids at bind as `BindTimeout` — today's rule, kept. The
execution lane schedules the finals it has: a class with none holds no permit and moves no other class's
permits (`palw_execution_schedule_snapshot_v1` over `PalwExecFinalV1`s — a `Provisional` or `PanelBound`
claim is not a final by construction, `record_round_final` runs only in `finalize_claim`).

**Decision 4 — capacity is sized by Little's law at 60–70 %.** `panel_load = accepted_rate × seat_count ×
service_time`; `utilization = load / (eligible_seats × span)`; the class is live when a panel can be drawn and
utilization is under one; it is *sized* when utilization is under the target with the outage margin
(`min_eligible_seats = seat_count + 2`: one operator down still draws a full panel, two down still runs). At
70 % a class absorbs one seat's loss (7 → 6 seats: 70 % → 82 %) without a queue forming; at 90 % it does not.
The inflight cap, the class target (ADR-0076's retarget is already the class-specific acceptance rate: a
class that outruns its panel should have its share, not its price, answered — ADR-0107, share by `Final`),
and the free collateral are the three limits, and the gate is what keeps the queue from growing when the first
two lag.

**Decision 5 — collateral.** Inflight duties hold `inflight × seat_count × seat_exposure`; at ADR-0130's λ = 2
(256 MSK a seat on the dense tier) a 10,000 MSK bond carries 39 duties, so a five-span window at 2.2 claims a
span holds 55 duties over 26 seats — two a seat, well inside — and a seat exposure two hundred times larger
needs more bonds than seats: the cap must hold before the collateral runs out, and the profile's cap is
derived so that it does (the census now prints each class's free collateral beside its duties).

**Decision 6 — the artifact is a runtime problem, priced apart.** Compute is `EconomicComputeV1`; the bytes a
replay moves are `artifact_bytes_fetched` (ADR-0132's telemetry: the MiB read from storage per draw and, with
the residency stats, the loader's hits and misses) and enter the *window* and the *prefetch*, never the
CCU. Of the eight options the operator listed (§4): **E + B + G + D** — a seat registers its readiness for a
class (E: capability by evidence, ADR-0132 F1: a receipt or a production on the class), holds the artifact as
content-addressed immutable blobs (B: the artifact root is already the class's registration fact), pages only
the active experts of a mixture (G: the hybrid's expert loader already does this under a residency budget — the
ibm host simply had none), and prefetches before the class is activated for it (D: `artifact_prefetch_spans`).
A after-draw full download (today's implicit design where the operator did not place the file) is what killed
the hybrid; C (chunked P2P) and F (a shared immutable cache on a host) are transports for B; H (trace and
checkpoint first) is the interval lane, which exists and is dead on the fleet (ADR-0132 G1).

**Decision 7 — a seat is a verdict, not a GPU.** Consensus binds a seat to a bond and a deterministic verdict
over a job; how the seat computes it — one GPU, tensor- or pipeline-parallel across several, CPU + GPU — is
the runtime's, provided the roots are bit-identical (the A16 tier and the hybrid's ops are integer, ADR-0049).
The reference rate is a floor on the *profile*, not a ceiling on the seat; a faster seat simply verifies more
duties. Nothing in the profile names hardware.

**Decision 8 — credits only from `Final`.** Unchanged and pinned: `record_round_final` in `finalize_claim`
only; the simulation's lane schedule is the real snapshot function over finals only, and a class with a
million bound claims and no `Final` holds no permit.

## 4. The artifact options, compared

| option | what it fixes | what it costs | verdict |
|---|---|---|---|
| A. full download after the draw | nothing; it is why a 24 GB class never licensed | 41 h of a dead panel per claim | no |
| B. content-addressed artifact cache | one artifact root, many hosts, deduplicated; the registration already names the root | a store per host | yes (the substrate) |
| C. chunked P2P distribution | a 300 GiB file reaches many seats without one server | a transport to build and secure | later, as B's transport |
| D. prefetch before activation | the cold case becomes rare; the window is sized on cold but paid on warm | seats must know the class ahead: the profile's `artifact_prefetch_spans` | yes |
| E. readiness registration by the operator | the draw stops dealing hopeless duties (ADR-0132 F1) | a fact per (bond, class); consensus-critical (draw eligibility) | yes, at a fence |
| F. shared immutable cache | several seats on one host page one copy | host layout | yes where it applies |
| G. lazy chunk fetch (active experts) | the hybrid's 24 GB becomes its working set; the loader exists | misses cost a read each; a residency budget must be set | yes — set the budget |
| H. trace/checkpoint first | a seat verifies intervals from openings instead of replaying whole | the opening lane must work (ADR-0132 G1) | yes, once it works |

## 5. Panel capacity and the queue

`panel_load = accepted_claim_rate × verification_time × panel_seats` (the operator's formula) is the model's
demand in seat-time; `eligible_seats × span` is the supply. The census now reports per class the eligible
seats, the seats on duty, the seat exposure they hold and the free collateral; the CLI derives the profile and
the utilization from them and from this node's replays. The queue's safe ceiling is the utilization at which
one operator's loss keeps it under one: `u ≤ (n − s)/n` for `n` eligible seats and `s` seats an operator holds —
5/7 = 71 % at seven seats, one seat an operator. 60–70 % is right for seven; a larger pool tolerates more.

## 6. The grid (the simulation, `palw_verification_sim_grid_v1`, at 2.2 claims a span, seven seats, warm)

| warm p99 | artifact | cold p99 | window spans | prefetch spans | inflight cap | utilization | seats at 70 % (+1, +2) | live |
|---|---|---|---|---|---|---|---|---|
| 60 s | 30 GiB | 92 s | 2 | 1 | 19 | 16 % | 5 (6, 7) | yes |
| 60 s | 100 GiB | 167 s | 2 | 1 | 19 | 16 % | 5 | yes |
| 60 s | 300 GiB | 382 s | 3 | 2 | 29 | 16 % | 5 | yes |
| 120 s | 30 / 100 / 300 GiB | 152 / 227 / 442 s | 2 / 2 / 3 | 1 / 1 / 2 | 9 / 9 / 14 | 31 % | 5 (6, 7) | yes |
| 240 s | 30 / 100 / 300 GiB | 272 / 347 / 562 s | 2 / 2 / 3 | 1 / 1 / 2 | 4 / 4 / 7 | 63 % | 7 (8, 9) | yes (73 % with one seat down) |
| 480 s | 30 / 100 / 300 GiB | 512 / 587 / 802 s | 3 / 3 / 4 | 1 / 1 / 2 | 3 / 3 / 4 | 126 % | 13 (14, 15) | **no** |
| 900 s | 30 / 100 / 300 GiB | 932 / 1,007 / 1,222 s | 4 / 5 / 5 | 1 / 1 / 2 | 2 / 3 / 3 | 236 % | 24 (25, 26) | **no** |

`Final` latency is the window plus 240 spans of challenge for every row — the challenge window, not
verification, is the latency. Throughput at 70 % of seven seats: 60 s → 9.8 claims a span; 120 s → 4.9; 240 s → 2.45; 480 s → 1.22;
900 s → 0.65. The inflight cap is `⌊0.98 × window / warm⌋` at seven seats and 70 %. **Full replay carries a 240-second class on seven seats at the live rate and no more; a 900-second
class needs 24 seats or a fifth of the rate.** With residency (prefetch) the cold column disappears from the
capacity but not from the window.

## 7. Where full replay ends

Full replay scales linearly in seats: a class that costs `T` seat-seconds a claim at rate `r` needs
`5 × r × T / (0.7 × 600)` eligible seats. Multi-span windows remove the deadline, prefetch removes the cold
term, multi-GPU seats shrink `T` by the parallelism a deterministic runtime can give (the arithmetic is
integer; tensor-parallel splits are deterministic if the reduction order is fixed). What does not scale: the
artifact must be *resident somewhere on every seat that replays it* (300 GiB × seats), and every claim is
verified five times whole. A class whose warm replay is an hour on the reference needs 96 seats at 2.2 claims
a span, or 0.05 claims a span on seven — at which point the class's share, not its price, is the answer, and
the second generation (§8) is the design question.

## 8. Second-generation designs (not adopted; compared)

| | security model | soundness | panel collusion | complexity | artifact | latency | hardware | consensus | migration | attack surface | weaker than 5 × full replay where |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **S1. Checkpointed segmented replay** — the producer commits per-segment state roots (the trace already has them, ADR-0082 D9); each seat replays `k` segments drawn after the bind, resuming from the committed checkpoint (ADR-0103's resume route) | a lie in a segment is caught by any seat that draws it; cache-altering lies by every seat | probabilistic: ADR-0098's number per segment | as today (3 of 5), but a colluding panel can license a lie in an undrawn segment | medium: the routes exist | the whole artifact, but a seat's *time* is `k/N` of the job | `k/N` of full | as today | segment count and `k` in the profile; a fence | none (claims unchanged) | grinding the segment draw (drawn after the bind — cannot) | an undrawn segment is unverified: security drops from 100 % of the job to `k/N` per seat, `1 − (1 − k/N)^5` per lie |
| **S2. Optimistic licensing with sampled audits and the court** — licence = no fault proven in the window; one seat replays whole (drawn), the rest sample; the court convicts (ADR-0093/0100 exist) | fraud proofs; the reservation is the bond | economic: a lie costs `claim.reserved` when caught, ADR-0098's odds | one replaying seat can be bribed to stay silent; the samplers still catch cache lies | low–medium: mostly rule changes | one full copy a claim, not five | short (a licence at the window's end without a quorum) | one replaying seat per claim | licence rule; a fence | none | silence (already unchargeable); a colluding full-replay seat | the licence no longer carries three whole-execution signatures: security rests on one replay + samples + the court |
| **S3. Deterministic trace commitment with succinct verification** — the producer commits every layer's activations (the trace root exists); seats verify a random subset of *layers* per position by re-running one layer from committed inputs (no full job, no full artifact — one layer's weights) | each check is exact for its layer; lies must pass all sampled layers | probabilistic per (position, layer); high per-claim | as S1 | high: per-layer openings, per-layer weight addressing (the named store already addresses tensors) | **a layer's weights per check**, not the artifact | short | small | opening format; a fence | claims unchanged, material format versioned | the trace size (every activation) | sampled, not whole; and the trace must be served (the interval lane's problem, ADR-0132 G1) |
| **S4. Cryptographic proof (zk/validity proof of the forward)** | proof soundness | exact | none needed | very high; no deterministic integer LLM prover exists at this size | prover-side only | prover time ≫ inference (hours) | prover hardware | new verifier | new claims | the prover's toolchain | not weaker if sound; not available |
| **S5. Producer-side redundancy** — two independent operators execute the same job (the second is the "seat"), quorum of two exact roots + one sampler | exact agreement of two executions | exact unless both collude | 2-of-2 collusion | low | two full artifacts | one replay | as today | panel size and quorum; a fence | none | the pair's collusion | fewer independent verifiers (2 + 1 instead of 5) |

**What is weaker, said plainly:** every design but S4 checks less than five whole re-executions. S1 and S3 keep
exactness per checked unit and trade coverage for cost, with the court behind them; S2 trades the licence's
signatures for fraud proofs; S5 trades independence for cost. None is adopted here; S1 is the natural next
step because its routes exist (checkpoints, resume, the interval draw) and its cost is a knob in the profile.

## 9. Activation of a Kimi-class model (the procedure)

1. **Register** the class with its graph (compute derives), its artifact root and `artifact_bytes`; share 0.
2. **Shadow** (ADR-0131 Decision 6): three operators place the artifact (B/F), set a residency budget (G), run
   seats; the node's ledger and telemetry measure replays warm and cold, the MiB read, the eligible seats.
3. **Calibrate**: `PalwVerificationProfileV1` from the measured p99s (the CLI prints it); read utilization at
   the intended share; confirm `min_eligible_seats` with the outage margin.
4. **Activate** at a fence: the profile fixed (window, prefetch, cap), the class's share granted, readiness
   registration (E/F1) required for its seats; the gate armed for the class.
5. **Watch** the census: utilization under 70 %, held claims zero, free collateral above the inflight exposure;
   raise the share only while that holds.

## 9a. The order of work (the operator's, 2026-09-17)

Four axes that never fork together: **Execution** (1 BPS, the fast UX), **PALW anchor** (~120 s, settlement
cadence), **Verification** (a class profile: short for the dense tier, long for a Kimi-class), **Economics**
(`EconomicAttempted` CCU, MSK per compute). And the absolute condition: a Kimi class stopping ≠ the dense tier
stopping ≠ execution stopping ≠ the anchor stopping.

* **Phase 0 — operations, before any fence:** the hybrid's artifact on three seat hosts that are not its
  producer; the `.t11b` node out of IBD; the `not-held` openings diagnosed. Goal: the hybrid `licensed > 0`,
  `Final > 0`. `actual MSK / attempted CCU = 0` is not a pricing bug and no rate is decided while it is zero.
  ADR-0132's and this ADR's shadow observability merge as they are.
* **Fence 1 — panel liveness + verification profile** (ADR-0132 F1/F2 with this ADR's profile, gate and
  capacity): `PalwVerificationProfileV1 { verification_window_spans, artifact_prefetch_spans,
  max_inflight_claims, required_ready_seats, warm_p99_ms, cold_p99_ms }` frozen at activation from the shadow
  benchmark (never recomputed in consensus from live measurements); **readiness by evidence, not by
  declaration** — a seat is `ready(class)` only with the artifact root matched, the manifest/chunks held, chain
  participation normal, free collateral, and a recent successful verification on the class; a seat that says
  `Incapable` does not get to say it again and again for free; class-local inflight cap; eligible-seat capacity
  gate; free-collateral gate; early redraw on `Incapable`/`Unavailable`; `NoCapablePanel`; a class-specific
  receipt/verification window; no compute credit before `Final` (already the rule, pinned).
* **Measure after Fence 1**, per model: actual MSK / attempted CCU, licence and `Final` rates, warm/cold
  verification p95/p99, artifact cache hits and bytes fetched, panel utilization, eligible seats, inflight
  duties, free collateral, reserved exposure. Targets: p95 utilization < 70 %, cap hits ≈ 0, `NoCapablePanel`
  ≈ 0, the protocol-caused difference between the two tiers' `Final` rates small, spare eligible seats ≥ 2.
  Not "identical `Final` rates" — but never a zero because an artifact is missing.
* **Fence 2 — decide the single lottery** (§5, ADR-0132 S): a separate ADR compares on devnet
  `EA_class × EA_network` against the class target carrying the cadence alone (`EA_network = 1`); decided
  **before** the economic payout forks, because the payout's `W = CCU_draw × EA_class × EA_network` loses the
  last factor under the single lottery, and a payout forked first would be re-forked at once.
* **Fence 3 — `EconomicAttempted` payout**, once liveness is normal and the lottery is decided: snapshot at
  acceptance (`draw_ccu`, `EA_class_q32`, `EA_network_q32` unless single-lottery, `economic_rate`); producer
  `actual MSK / attempted CCU ≈ constant`; panel `panel MSK / verification CCU ≈ constant`, the two CCUs never
  mixed. **`min(escrow, CCU × rate)` is not the mainnet design as it stands:** a Kimi-class whose economic
  payout is 7,000 MSK against a 3,200 MSK escrow cap is capped and its MSK / CCU falls again. Before any class
  is activated as paid, the shadow shows `uncapped_economic_reward`, `capped_reward`, `cap_utilization` and
  `cap_saturation_rate`; a class above 80 % cap utilization is not activatable. Long term the three cannot all
  hold unconditionally — MSK / CCU constant, total issuance constant, any model claiming at any frequency —
  so a heavy class is a high reward per claim at a low claim frequency: economic CCU (a claim's value), class
  DAA / admission (its frequency), a global issuance budget (the total).
* **A Kimi-class, step by step:** register (share 0, artifact root, bytes, graph) → artifact prefetch on
  3–N operators, readiness confirmed → shadow (warm/cold p99, verification CCU, utilization, collateral,
  `EconomicAttempted`, capped/uncapped reward) → profile freeze (window, cap, `required_ready_seats`,
  prefetch) → devnet (operator outage, cache miss, the heavy class at 90 %, collateral exhaustion, recovery)
  → tiny activation (a small share) → ramp only while utilization < 70 %, `NoCapablePanel` ≈ 0, `Final` rate
  normal, no cap saturation. A class holds no share the moment it registers.
* **Full replay stays to a warm p99 of ~240 s** (seven seats); 480 s is a 13-seat class, 900 s a 24-seat one —
  past that the verification architecture changes rather than the seat count. **Segment replay is
  "PALW Verification V2"**, its own ADR: V1 = full replay + multi-span + prefetch + readiness + capacity
  gate; V2 = segment commitments + random segment assignment + one full-replay seat + court/fraud proof. A
  Kimi-class that runs under V1 does not need V2.
* **Not in this fence:** the DNS/VLT committee-beacon retirement and the ADR-0125 drill — a different failure
  domain, their own ADR and their own fence (ADR-0134), so a failure at either height names its cause.

## 10. What is built (shadow)

`consensus/core/src/palw_verification_profile_v1.rs`: the facts, the reference, the profile derivation, the
capacity arithmetic, the class-local gate, the network simulation over the real lane snapshot, and the grid.
The census (`palw_class_census_v1`) now carries each class's eligible seats, seats on duty, the exposure they
hold and the eligible bonds' free collateral; op 185 and `misaka palw economics` print them with the derived
profile and the utilization. Nothing consensus reads. Not built: the fence and the fold-side gate (Decision 3's
site is named), readiness registration (ADR-0132 F1), the receipt-window per class (today's 600 DAA already
exceeds every profile in §6; shortening it is ADR-0130's next ADR).

## 11. Implementation record

* 2026-09-17, `feat/palw-exec-lane-and-validator-retirement`. Tests (`adr0133_*`, `palw_verification_profile_v1`):
  the profile is derived and cannot be shrunk (the dense tier one span, the hybrid two, the Kimi stand-in five;
  under-reporting floored at the estimate); Little's law sizes the panel (5 % / 236 %, 24 seats, the rate at
  70 %, cold replays cost more); the gate is class-local; **a Kimi-class starvation stops only Kimi** (120 grid
  cases: Qwen only, Kimi only, both, one-span beside five-span, slow 90 %, fast 90 %, one and two operators
  down, warm and cold caches, 100 and 300 GiB — Qwen's every count identical with and without Kimi, Kimi
  licensed exactly when its own arithmetic says it is live, the real lane snapshot schedules exactly the classes
  with a `Final`, the anchor cadence untouched, recovery with the seats the profile needs, the cap holding the
  surplus); collateral bounds the inflight before time does (exhaustion at 200 × λ = 2); the grid is monotone
  and is what §6 prints. Verification: consensus-core 2,149 / rpc-core 148 / misaka-cli 176 / rpc-service 1 /
  kaspad 98 / integration `rpc_tests::sanity_test` 1 — all green; clippy `-D warnings` over the eleven crates
  clean; `shipped_presets_have_pinned_fingerprints` unmoved (`ab4e7b9c…`).

## 12. Number hygiene

0133 was free when written; the next free number is 0134.
