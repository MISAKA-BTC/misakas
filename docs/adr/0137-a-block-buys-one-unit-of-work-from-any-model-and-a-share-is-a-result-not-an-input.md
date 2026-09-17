# ADR-0137 — A block buys one unit of work from any model, and a share is a result, not an input

**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`, design with a
reproducible simulation (`scripts/palw-share-sim.py`); nothing armed, no fingerprint moves. The
interim commit `804af11d` (a class the registry seats is priced by `attempt_target_seed_v1`) stays
as a shadow of the shipped rule and is **not** the final design — §6 says why.

**Supersedes the share-as-input reading of:** ADR-0045 D3 (the grant table), ADR-0039 D5 (epoch
budgets), ADR-0071 D1 / ADR-0076 (the class DAA and its seed), ADR-0054 / ADR-0107 (share growth,
dormant), ADR-0135 D5 (shares from admission) and ADR-0132 Upgrade C's cap rule (ADR-0133 Fence 3).
It keeps: the registry's lifecycle as a *verifiability* gate (ADR-0135 D6), the panel share rule
(ADR-0132 C's `clamp(α·C_V/(C_P+α·C_V), 10 %, 30 %)`), the execution lane's Final-credit quotas
(ADR-0125), the economic compute measure (ADR-0131) and the free-prompt lane split.

**Asked (2026-09-18):** redesign "model share" itself. Anyone may register a model; a model that
brings more real compute should hold more of the network, without a person setting `Qwen = 40 %`;
a small and a large model must earn the same MSK per Economic CCU; share should mean *what fraction
of PALW compute the model actually provided*; suspect first whether share must be a lottery input
at all; find the positive feedback, the cold-start trap and the registration Sybil; compare at
least three designs with simulation; define "usage"; check the issuance identities; keep execution
permits apart; say whether the `share` consensus parameter can be deleted.

## 0. The sentence this ADR is

**Every block pays the same escrow for the same expected amount of work, from whichever model did
it; the network holds one work target `W` (CCU per block) the way it holds `bits`; a model's ticket
is `CCU_m / W` and needs no share, no class target, no budget and no seat price; the floor fills
whatever cadence the models leave and is paid nothing for it; and "share" is the fraction of
finalized work a reader computes, never a number the chain feeds back into anyone's odds.**

## 1. Where share sits today — the formulas as shipped

Notation: `CCU_m` the economic compute of one forward of class `m` (MAC-eq, derived from the graph:
`PalwModelWorkV1::economic_ccu_per_claim`, ADR-0131); `r_m` forwards per second its producers run;
`C_m = r_m · CCU_m` the compute it supplies; `E` the block's escrow (the worker carve of the subsidy,
2,756.28 MSK on testnet-11); `p_m` the class ticket's probability a forward; `p_net` the network
draw against `bits`; `s_m` the class share in permille.

1. **The ticket** (`check_palw_class_lottery_v3`): a forward admits iff
   `class_ticket_v3(attempt) ≤ T_m`, so `p_m = (T_m + 1) / 2^128`; the block then also passes the
   Layer-0 digest against `bits`, `p_net = (target + 1) / 2^256` (ADR-0132 §1.1: two draws, one
   credited).
2. **The class DAA** (`retarget_over_span_v1`, every epoch): `expected_m = s_m · total_blocks`,
   `T_m ← clamp(T_m · expected_m / observed_m, ÷4, ×4)`; a class with `observed = 0` is skipped and
   only `converge_idle_target_v1` moves it, toward the hardest price a *producing* class pays and
   never past it. Shares are renormalised over the classes that produced in the closed epoch.
3. **The epoch budget** (`derive_epoch_budgets_v2`): `budget_m = max(1, epoch · s_m · tol /
   census)` blocks, census = the shares of the classes that produced last epoch; a block past the
   budget is refused (`EpochBudgetExceeded`); ADR-0123 releases what the others were due and did
   not produce, progressively.
4. **The grant table** (`granted_share_table_v2`): the first class is the floor at 1000 ‰, every
   later registration is funded by every incumbent pro rata, at least `min_grantable_share_permille`
   (`⌈10⁶ / (tol · epoch)⌉`), the floor never below `min_base_class_share_permille`.
5. **The registry's shares** (ADR-0135 D5, `step_model_registry`): every governed boundary,
   `s_m = room · admission_m / Σ admission`, `admission_m = min(budget_ccu / CCU_m, inflight_cap_m /
   window_m) · {50, 100, 1000} ‰ by state`, `room = 1000 − floor`; the floor holds the rest.
6. **The seat price** (ADR-0076, and `804af11d` for the registry's admission):
   `T_m = MAX · s_m · pwu_m / 2^31`.
7. **The reward**: the block's escrow `E` at 80/20 (ADR-0124); under Upgrade C
   `min(E, attempted_m · rate)` with `attempted_m = CCU_m / (p_m · p_net)` in expectation, the panel's
   share by `α·C_V / (C_P + α·C_V)`, and a class whose `attempted · rate / E > 800 ‰` is not
   activatable (Fence 3).

So the reward a model earns per unit of compute is, from 1, 2 and 7:

```text
ρ_m = MSK / CCU = 0.8 · E · p_m · p_net / CCU_m                      (escrow whole)
    = 0.8 · min(rate, E · p_m · p_net / CCU_m)                       (Upgrade C)
```

and the class DAA sets `p_m` so that `r_m · p_m · p_net = s_m · BPS`, i.e. **`ρ_m = 0.8 · E · s_m ·
BPS / C_m`**: a model is paid in proportion to its *share* and in inverse proportion to the compute
it brings. Two models with the same share and different supply are paid differently per CCU; two
models with the same supply and the shares the registry hands them (§3.3) are paid in inverse
proportion to their cost. Equal pay per CCU holds only where `s_m ∝ C_m` — which is the thing the
share table cannot know without measuring `C_m`, and measuring it is the feedback loop of §3.

## 2. Why the ticket was 3.589 × 10⁻³

Op 180's terms (`palw_v2_registration_terms_impl`) hand a registrant `initial_target = the floor's
current target`. The floor's target is what its own class DAA converged to for microsecond integer
draws — on the devnet `p = 3.589 × 10⁻³`, and node-3's own floor producer printed that exact number
(`class ticket p = 3.589e-3`) before it was restarted as the class producer and printed it again for
the class. A Qwen2.5 forward takes tens of seconds on that host, so the copied price meant hours a
claim. Nothing re-priced it: the registry holds a REGISTERED/PREFETCHING class at share 0 and
grants 980 ‰ at admission, but the class DAA skips a class that produced nothing, the idle
convergence moves an idle class only *toward* the incumbent's price (which it already had), and no
rule reads the share into the target. The earlier reading — over-production cutting the target —
was wrong: the fresh-genesis run with the budget fix showed the same ticket. `804af11d` prices the
class at its seating by ADR-0076's seed (`980 ‰ · 1,589,424 / 2^31`, saturating to `p = 1.0`), and
the drill's phase 2 accepted the class's first claim three minutes after the producer started.
That fix is correct *within* the share→target design; §3 is why the design is the problem.

## 3. The loops and traps in share → target

Simulated in `scripts/palw-share-sim.py` (§20) with the shipped arithmetic: the profile globals,
the lifecycle machine, the class DAA with integer block counts, the competing-census budgets with
ADR-0123's release, the floor as its own class, a network-wide panel that replays every claim at
the reference rate and voids what waits past its window. Supply is stated relative to what the
panel can verify, because on testnet-11 verification binds long before compute does.

### 3.1 Cold start — permanent, not slow

A registration copies the floor's price (§2). `A0` in every scenario: the model's ticket stays at
`3.8 × 10⁻⁵` for 120 epochs, `10⁻⁵` claims a span, PROBATION forever (it never collects ten probes).
`converge_idle_target_v1` cannot help — its doc says so ("a class seeded at the floor's price is
already at it, so it does not move"). The trap is structural: the only rule that eases a silent
class eases it toward a price it was born at.

### 3.2 The lifecycle sawtooth

`palw_lifecycle_step_v1`: `utilization ≥ 1000 ‰ → HELD` from PROBATION, ACTIVE_LIMITED and ACTIVE
alike; `HELD → PROBATION` (admission ÷ 20, probes reset) `→ ACTIVE_LIMITED` (÷ 10, three stable
spans) `→ ACTIVE`. Utilization is claims in flight over the panel's window capacity, and the
cadence a share hands a class (`s_m · BPS`, 2.45 blocks a span at 490 ‰) is *unrelated* to the
capacity the share was derived from (about one claim a span for either Qwen on eight seats). So a
class the registry admits in full overloads its own panel by construction and is HELD; the surge
scenario (`Q36 supply × 10`) walks `ACTIVE → HELD → PROBATION` and pays Q36 `2.97` against Q25's
`7.20` MSK per G-CCU afterwards. A bang-bang controller with a ÷20 reset is an oscillator, and the
thing it oscillates is a model's income.

### 3.3 The inverse-cost share

`admission_m = min(budget_ccu / CCU_m, inflight_cap_m / window_m)`: both terms fall with `CCU_m`,
so **the cheaper the model, the larger its share**. A class of `0.5 G` MAC-eq is admitted at
`44` claims a span against Qwen2.5's `0.26` — 170 × the share for the same supply. With §1's
`ρ_m = 0.8 · E · s_m · BPS / C_m` that is 170 × the pay per CCU; the simulation prints a `ρ`
spread of `4.02` between the two Qwens (their cost ratio) and `169` with the tiny class. This is
exactly the "small model wins because it is fast" the redesign forbids, and it is the shipped
Upgrade A rule.

### 3.4 The cap trap (Upgrade C + the registry) — a positive loop

Fence 3: `attempted_m · rate / E > 800 ‰ → ACTIVE_LIMITED`. `attempted_m = CCU_m / (p_m · p_net)`.
ACTIVE_LIMITED divides the class's admission by ten, so its share and its expected blocks fall,
so the class DAA *hardens* `T_m` (fewer blocks expected of it), so `attempted_m` rises tenfold, so
its cap utilization rises tenfold, so it stays limited. The rule punishes a model for being heavy or
popular and then makes the punishment permanent. (The simulation's `A1` reproduces the mechanism
qualitatively; the arming branch's numbers put the dense class at 69.9 % of the escrow, one supply
surge from the ceiling.)

### 3.5 The cadence collapse

Shares from admission give the models `room = 980 ‰` whether or not their producers can fill it.
Once a model produces at all it enters the competing census, the floor's renormalised share is
`20 ‰`, and the floor's class DAA hardens its target by ×4 an epoch until it produces 2 % of the
total — of a total the models cannot fill. Simulated (`A1`, every steady scenario): `0.25–0.57`
blocks a span against a 5-block span, i.e. the chain at 5–11 % of its cadence, the floor at
`2.28` blocks an epoch, until the floor's expectation rounds to zero and its retarget stops. The
devnet at 0.05 BPS cannot show this inside the drill's window; a 1 BPS network would. **Arming
ADR-0135 D5 as shipped on testnet-11 is a liveness hazard**, independent of everything else here.

### 3.6 The dormant usage loop

ADR-0054 / ADR-0107 (`derive_class_share_growth_v1`, `palw_share_growth_final`, dormant) would add
`production → share`: with §1's target rule that is the loop the redesign asked about — `share ↑ →
target easier → more claims → more production → share ↑` — bounded only by the growth cap and the
floor. It is not armed; it should not be.

### 3.7 What is *not* a loop

The shipped share (5) does not read usage, so there is no positive feedback in `A0` — there are the
five traps above instead. The retarget itself is stable (its expectation sums to what happened,
ADR-0071's H1 fix), which the simulation confirms: `A1`'s block shares settle in ≤ 10 epochs where
nothing else intervenes.

## 4. Permissionless registration under the shipped rule — the floor-share Sybil

Register `N` classes (the same weights, `N` variants, `N` graphs a node apart). Each gets:

* a class target — the floor's price (§2), so each draws like a second floor;
* an epoch budget of **at least one block** (`derive_epoch_budgets_v2` floors at one, share 0
  included: "a weightless class still produces"); `N = 1000` is ten epochs of cadence a span;
* no gate at acceptance: `palw_admission_v2` refuses a bad ticket, a spent budget and a missing
  target — it does not refuse a class in REGISTERED/PREFETCHING/HELD; the claim voids later
  (`NoCapablePanel`) but the block stood, the network draw hardened for everyone, and the honest
  classes' cadence was diluted;
* if admitted, share ∝ 1 / CCU (§3.3): a thousand cheap variants own the room.

Cost: the registration bond (`registration_bond_per_span · window`, 1,000 MSK a span-window) —
locked, released at reclamation. **A bond prices classes, not blocks**: it bounds how many classes
an attacker holds, not how much cadence each one takes, and a refundable bond is an interest cost.
It cannot close this on its own; only removing the per-class allocation does.

## 5. "Usage", defined — which CCU may mean what

| quantity | where it is on the chain | what distorts it | may consensus read it? |
|---|---|---|---|
| **attempted CCU** | not an event: an *expectation*, `CCU_m / (p_m · p_net)` from the target | none, but it is a model of the producer, not an observation | only as this expectation (ADR-0132's snapshot does exactly that) |
| **accepted CCU** | every attempt block | forgeries and no-panel claims count until they void | for a cadence census, not for money |
| **licensed CCU** | `ReceiptLicensed` | the panel's liveness (a dead seat licenses nothing) | yes, delayed |
| **Final CCU** | the claim's `Final` | the panel's liveness *and* the window; a HELD class finalizes nothing | yes — the strongest, the latest |
| **paid CCU** | the payout rows | the price (Upgrade C's min) — it measures MSK, not work | no, it is the answer, not the question |

**Decisions.** (a) The lottery reads none of them: a ticket is priced from `CCU_m` and `W` (§7).
(b) Under §7 every accepted claim represents `W` of work in expectation, so `attempted CCU` becomes
*exactly observable*: it is `W` at acceptance, snapshotted as ADR-0132's row already does.
(c) The model share a reader reports is **rolling Final work**: `share_m = Σ_{Finals of m in the
window} W_accept / Σ_{all Finals} W_accept` — the fraction of *verified* work, which is what the
question "what fraction of PALW compute did this model provide" means; a HELD or unverified class
drops out of it, correctly. (d) Nothing off-chain (a producer's own draw count, a seat's wall clock)
enters any of it.

## 6. The candidates

| | A — shipped, dynamic class share | A′ — `804af11d` + Upgrade C | B — global normalized lottery | C — B, share for statistics and execution only | **D — the work target (this ADR)** | E — D with headroom-scaled tickets |
|---|---|---|---|---|---|---|
| lottery input | share → class target (DAA) | share → seat price → DAA | `CCU_m / W` | `CCU_m / W` | `CCU_m / W`, `W ≥ W₀`, one DAA over model blocks | `CCU_m / W · (1 − load)` |
| reward a claim | `E` | `min(E, attempted · rate)` | `E` | `attempted · rate` (= `W · rate`) | `E` (and Upgrade C's formula degenerates to `E`) | `E` |
| MSK / CCU across models | `∝ s_m / C_m` — spread 4.02 (Qwens), 169 (tiny) | equal only while uncapped; the cap trap | equal, `E / W` | equal | **equal, `0.8 · E / W`, every scenario** | unequal for capacity-bound classes (2.02) |
| cold start | permanent (floor price) | seat price, then §3.2 | none needed | none | **none needed** | none |
| Sybil by classes | cadence per class (§4) | same, for unadmitted classes | none: a block costs `W` whatever the class | none | **none; 1000 classes = 1 class** | none |
| cadence | collapses to the models' rate / 0.98 | same | collapses without a residual floor (0.37 of 5 blocks a span) | as B | **held: the floor is the residual** | held |
| feedback | sawtooth, cap trap | same | none | none | **none (one global DAA)** | headroom loop (mild) |
| consensus state a class | share, target, budget, row | + priced share | row | row | **row only** (CCU is in the registration) | row + load |
| human-set numbers | shares, floors, seeds | + rate | `W₀` or nothing | rate | **one: `rate_max` (= `E / W₀`)** | one |

`B` is `D` without the residual floor: with `W` walked over every block and no floor rule, the
cadence at testnet-11's supply is 0.37 blocks a span. `C` is `D` with the reward written as
`attempted · rate`; under a work target `attempted = W` for every claim, so `C ≡ D` once `rate` is
read as `E / W` — a derived number, not a second constant. `E` scales a class's ticket by its
verification headroom instead of holding the producer: it under-pays exactly the classes the panel
cannot keep up with (their forwards mostly lose) and adds a loop; the producer's pre-check wastes
nothing. Rejected. **D is recommended.**

## 7. The design — one work target

**D1 — the ticket.** A class has no target. At a block with work target `W`,

```text
T_m = MAX · min(1, CCU_m / W)          p_m = min(1, CCU_m / W)
```

with `CCU_m` the registration's economic compute per claim (`PalwModelWorkV1::economic_ccu_per_claim`,
derived from the graph, the same on every node). The expected forwards a win is `W / CCU_m`, so the
expected *compute* a win is `W` for every model with `CCU_m ≤ W`.

**D2 — the work target.** `W` is one chain value, walked like `bits`, over model blocks only:

```text
at every epoch boundary:
    W ← max( W₀ , clamp( W · model_blocks / expected_blocks , ÷4 , ×4 ) )
    expected_blocks = BPS · epoch_seconds
    W₀ = E / rate_max
```

`rate_max` is the one constant a network states: the most it will ever pay for a unit of compute
(the arming branch's `rate_sompi_per_giga`, 9 MSK per G MAC-eq, is exactly this number and needs no
recalibration). Below `W₀ · BPS` of supply the network pays `rate_max` and the models fill
`C / (W₀ · BPS)` of the cadence; above it `W = C / BPS`, the models fill the cadence and the rate is
`E · BPS / C`. `W` is a fold value (computed from the epoch counters the state already keeps), not a
new consensus object.

**D3 — the floor is the residual and it is unpaid.** The floor class draws against its own target
as today, walked to fill `BPS − model block rate`; the existing `DifficultyManager` on `bits` already
does this once model blocks are counted in the cadence (§17 keeps the network draw for now, so
`bits` needs no change). A floor block carries transactions and fees and no escrow. Nobody grinds
the floor for money, so a floor grinder cannot inflate `W`; the floor is liveness, not income.
(ADR-0039 W6′ wanted "no spam-hash lane could ever take the network": under D the floor cannot
take *issuance* at all, which is the stronger statement.)

**D4 — the reward.** A Final pays `E`, split producer / panel by ADR-0132 C's panel share with
`C_P = W` at acceptance. `ρ_m = 0.8 · E / W` for every `m` — the fairness goal by construction.
Upgrade C's `min(E, attempted · rate)` with `attempted = W` and `rate = E / W₀ ≥ E / W` is `E`, so
the shipped payout code is unchanged and its snapshot rows remain the observability of "how much
work this claim stood for". Fence 3's cap rule is retired (under D its utilization is `W / W₀ ≥
1000 ‰` for everyone: it would limit every class).

**D5 — the verification budget replaces the per-class caps.** A claim of class `m` is accepted iff
the class is in an admitted lifecycle state and

```text
room_m = ( ready_seats · reference_work · utilization · window_m  −  Σ_k inflight_k · CCU_k · seats ) / (CCU_m · seats)  >  0
```

— one network-wide replay budget less what every class already holds in flight, in units of this
class's claims. Per-class in-flight caps do not sum to the panel (§3.2, and the simulation at ten
times capacity shows the cheaper class starving while both are "under their cap"); the budget does.
The producer pre-checks it (no forward is wasted); `NoCapablePanel` and the window void as today.

**D6 — the lifecycle is a gate, not a price.** REGISTERED / PREFETCHING / HELD: claims refused at
acceptance (a new error, the gate §4 found missing). PROBATION: one claim in flight, ten Finals to
leave — a probe costs `W` of real compute, so it is Sybil-resistant. ACTIVE_LIMITED / ACTIVE: D5's
room only. `admission_permille` (50 / 100 / 1000) has no reader.

**D7 — share is a reader's number.** `share_m` is §5(c), computed by op 185/186 and the CLI from the
Finals in any window the reader chooses; it is not written to the state and nothing consensual reads
it. The execution lane keeps its own Final compute credit and quotas (ADR-0125: 45 % a domain, no
consecutive rounds) — economic share and execution share are different numbers about different
things, and `Economic share = 70 %, Execution share = 45 %` is allowed by construction.

**D8 — the network draw stays, for now.** Two draws waste forwards (ADR-0132 S) but do not bias
between models (`p_net` is common), so D is fair with or without it; folding it away is a separate,
fork-choice-touching change (§17) and is not needed for this ADR's goals.

## 8. Issuance — what can and cannot be fixed at once

Let `I` be issuance a second, `ρ_m` MSK per CCU of model `m`, `C = Σ C_m`.

1. **Equal pay:** `ρ_m = ρ` for all `m`.
2. **Fixed issuance:** `I = const`.
3. **Free compute:** `C` is whatever registrants and producers bring, unbounded.

`I = Σ ρ_m · C_m = ρ · C` under (1). With (2) and (3), `ρ = I / C` must fall as `C` grows, so a
*fixed* `ρ` (a rate constant that never moves) is incompatible with (2) + (3) — Bitcoin's arithmetic.
(1) is compatible with both: it is an equality *across models at an instant*, not a constancy over
time. Hence:

* the controller is one global variable — `W` (equivalently the effective rate `E / W`) — and it is
  a *feedback controller on model block count*, the same shape as the difficulty adjustment;
* a per-model share can never be that controller: it moves the *split* of a fixed cadence, and the
  split is a different quantity from `ρ`; making `ρ` equal through the split requires `s_m ∝ C_m`,
  i.e. measuring `C_m` with a lag and feeding it back (§3);
* `admission rate` and `global economic rate` are one number here: `rate = E / W`, `W ≥ W₀`.

Regimes: `I = min( rate_max · C , E · BPS )` (the floor's blocks are unpaid and complete the cadence
in the first regime). On testnet-11 today (`C ≈ 4.5 G MAC-eq/s` verifiable, `W₀ = 306 G`) the models
fill ~1.5 % of the cadence and issuance is `rate_max · C ≈ 40 MSK/s`; the simulation's
`issuance/epoch` column prints it (4,032 MSK an epoch at 1 × capacity, 403 at 0.1 ×, 20,000 at 10 ×
supply of the cheap class).

## 9. Fairness — the proof and the one limit

For `CCU_m ≤ W`: a forward wins with `p_m = CCU_m / W`, the expected compute a win is `W`, every win
pays `E`, so `ρ_m = 0.8 · E / W` — independent of `m`, of `r_m` and of how many classes exist. The
simulation prints `7.200` for every model in every steady scenario of every design D row, and a
`ρ` spread of `1.00`.

The limit: a model whose single forward exceeds `W` (`CCU_m > W`) wins every draw and is paid `E`
for `CCU_m > W` of compute — under-paid by `W / CCU_m`. One block cannot pay for more than one
block's work. It binds only while the network is small (`W = W₀` and a forward heavier than
`306 G` MAC-eq, i.e. no shipped class today; Qwen3.8-27B at 198.7 G is inside); it recedes as `C`
grows (`W = C / BPS`) and with BPS (ADR-0125). It is stated, not hidden: op 186 prints `CCU_m / W`.

Claim frequency differs by design and is fine: Qwen3.6 claims four times as often as Qwen2.5 and
is paid the same per claim; a 300 G Kimi claims a fifth as often as Qwen2.5 and is paid the same
per claim; per CCU all three are equal (the `20/30/50` scenario).

## 10. A new model — no bootstrap, no free share

Its ticket is `CCU_m / W` from its first admitted forward; the `new model at epoch 60` scenario
prints Kimi at its compute share (`0.200`) and `ρ = 7.200` in the epoch it is admitted. Registration
buys nothing but a row and a bond; PROBATION costs ten claims' worth of `W` compute — the same
compute an honest class spends producing — so it is Sybil-resistant without any share. Under the
shipped rule the same scenario prints Kimi in PROBATION at `ρ = 0.474` (A′) or at the floor's price
forever (A).

## 11. Windows — no longer consensus-critical

Under D no consensus rule reads a rolling share, so the window is a reader's choice: op 185/186
should print Final work over the last 10 and 100 spans and an EMA (`α = 1/32`), which the
scenarios (`0 → 50 %`, `50 → 0 %`, a new model, a `× 10` surge, an outage, HELD and back) show as
monotone and history-free after one window. The only consensual window that remains is the
execution lane's Final credit window (ADR-0125), which this ADR does not touch.

## 12. Execution permits

Unchanged. The lane reads Final compute credit and caps a domain at 45 % with no consecutive
rounds; that is *consensus power*, bounded by construction. Economic share (§5c) is *income*, and
may be 70 %. The two must never be one number, and under D there is no share number that could be
mistaken for both.

## 13. A hundred models

State a class: the registration (already), a lifecycle row (already), no target, no share, no
budget: ~200 bytes less than today. The boundary: one `W` update and `N` room checks (`O(N)`), no
grant-table sort, no per-class retarget. The panel: one budget over `N` classes (D5), so a hundred
cheap classes cannot each claim "under my cap" and jointly drown the seats. `W` does not depend on
`N`. The simulation's `Q36 as 100 classes` and `1000 tiny classes` rows are numerically identical
to their one-class rows under D; under A′ the hundred copies of Qwen3.6 push both Qwens into
PROBATION at `ρ` 0.18 / 1.27.

## 14. Attacks

| attack | shipped rule | D |
|---|---|---|
| register 100 / 1000 classes | cadence per class (§4), share ∝ 1/CCU | nothing: a block costs `W` from any class; a class without a panel is refused at acceptance (D6) |
| minor variants, graph a node apart | as above, each a new floor-price class | each priced by its own `CCU_m`; a variant that claims more CCU than it runs is a *graph*, and the panel replays the graph |
| cheap floor grinding to depress model pay | the floor holds 2.2 % by share | floor blocks are unpaid and outside `W`'s DAA: no effect on `ρ` |
| a surge to overload a rival's panel | HELD → PROBATION for the rival (§3.2) | the surge's own claims are refused by the budget (D5); a rival is throttled, not reset |
| a heavy model to pull the rate | Fence 3 limits it, then traps it (§3.4) | its ticket is `CCU_m / W`; it earns `E / W` like everyone |
| forged CCU (a graph that says more than it does) | as today | as today: `CCU_m` is derived from the graph the panel replays; a claim's work is what the replay costs |
| a producer's private draw count | never on chain | never on chain (`W` is the only work number) |

## 15. Migration — at a height, nothing regenerated

At the fence (`Params::palw_work_target: Option<ForkActivation>`, dormant everywhere; the same
family as `palw_model_registry`):

1. `W := E / rate_max` at the fence block (the payout fence's `rate_sompi_per_giga`).
2. The state's `class_targets`, `class_shares`, `epoch_budgets` are no longer written or read past
   the fence; they stay in the encoding and the root (a field never leaves — the DnsParams rule);
   readers print them as "pre-fence".
3. The registry step stops deriving shares and re-seeding budgets (`804af11d`'s seating and the
   sixth finding's budget refresh become unreachable past the fence and are removed at the next
   cleanup).
4. The floor's class DAA keeps running for the floor only (its target is the residual).
5. `check_palw_class_lottery_v3` reads `W` and the class's CCU; a floor attempt reads the floor's
   target.
6. Fence 3's cap rule is not evaluated past the fence.
7. Old nodes: the fork-id gate names the height (ADR-0120's rule: a fence at a scheduled height is
   invisible to the fork-id unless its height is distinct — pick one).

No regenesis; the devnet drill re-runs from genesis as it does for every fence.

## 16. Consensus change scope

* `palw_state_v2.rs`: the epoch boundary (a `W` update; skip class retarget / idle convergence /
  budget derivation / share growth for non-floor classes past the fence); the attempt acceptance
  (the lifecycle gate; the room check); the registry step (no shares past the fence); a `work_target`
  fold value in the state (in the root).
* `palw_admission_v2.rs`: `check_palw_class_lottery_v3` reads `W` (a class's target is derived); the
  budget check bypassed past the fence.
* `palw_model_registry_v1.rs`: no arithmetic change; `admission_permille` unread; the row loses
  nothing (the `priced_share_permille` of `804af11d` becomes "pre-fence").
* `config/params.rs`, `fork_id_v1.rs`: the fence, its validation (needs `palw_model_registry` and
  `palw_economic_payout` at or below it), the fork-id row.
* `kaspad/src/palw_producer.rs` / `palw_panel.rs`: the pre-check reads the room; the producer
  prints `CCU_m / W`.
* RPC: op 186 prints `W`, `CCU_m / W`, the room; op 185 prints rolling Final work shares.

Untouched: the ticket hash, the panel, licensing, the court, the execution lane, the free-prompt
lane, ADR-0132's snapshot and payout arithmetic, the memory budget (ADR-0136).

## 17. The smallest implementation, in order

1. **Shadow (no fence, this week):** `W` computed in the fold and printed (op 186) beside every
   class's `CCU_m / W`; the rolling Final work share on op 185; the pre-check's room as a log line.
   Measures on testnet-11 what §8 predicts before anything is armed.
2. **The fence:** D1, D2, D4 (retire Fence 3), D6's acceptance gate, the frozen fields (§15). ~400
   lines in `palw_state_v2.rs` / `palw_admission_v2.rs`, one param, pins.
3. **D5's budget** replacing the per-class in-flight cap (~150 lines; can ship with 2).
4. **Later, separately:** ADR-0132 S (one draw) — touches block validation and blue work; not
   needed for fairness.

Do not ship §15 on testnet-11's 6,001 flag day: ADR-0135 D5 as shipped is the §3.5 hazard, and D
is the replacement. Keep the registry fence (Upgrade A's rows, proofs and gate) and arm the work
target with it, or arm neither.

## 18. What becomes legacy

| legacy | today | after D |
|---|---|---|
| `class_shares`, `granted_share_table_v2`, `min_grantable_share_permille`, `min_base_class_share_permille` | consensus | frozen at the fence, encoding kept, unread |
| `class_targets`, `retarget_over_span_v1`, `converge_idle_target_v1`, `attempt_target_seed_v1`, `class_daa_max_factor` | consensus | the floor only |
| `epoch_budgets`, `derive_epoch_budgets_v2`, ADR-0123's release, `budget_tolerance_permille`, `EpochBudgetExceeded` | consensus | frozen, unread |
| ADR-0054 / ADR-0107 share growth, `palw_share_growth_final` | dormant | delete |
| ADR-0135 D5 shares from admission, `admission_permille`, `palw_class_shares_from_admission_v1` | consensus (dormant fence) | unread past the fence |
| `804af11d`'s `priced_share_permille` and the seating | consensus (dormant fence) | shadow only; frozen past the fence |
| `ClassRegistered.share_permille`, `initial_target`, op 180's `initial_target` | registration | ignored past the fence (a registration carries its CCU) |
| Upgrade C's `min(E, attempted · rate)`, the snapshot | consensus (dormant fence) | kept — degenerates to `E`; the snapshot stays as the work record |
| Fence 3's cap rule, `cap_utilization_permille` | consensus (dormant fence) | retired at the fence |
| the network draw against `bits` for model blocks | consensus | kept (D8); ADR-0132 S later |

## 19. Can the `share` consensus parameter be deleted?

Yes. Every consensus reader of a share is one of: the class DAA's expectation, the epoch budget,
the grant table's conservation, the registry's derivation, the seed. Under D each has either no
reader or a reader that is the floor's residual rule. What remains that *looks* like a share:

* the **floor's cadence** — not a share but the residual `BPS − model rate`, and unpaid;
* the **free-prompt lane split** (`fp_attempt_share_permille`) — a lane split between attempt and
  receipt blocks, not a model share; out of this ADR's scope and unchanged;
* the **execution quotas** — Final credit with a 45 % cap, consensus *power*, not income.

The number a person could still set is `rate_max` (equivalently `W₀`), and it is a ceiling on pay,
not a split between models. No rule says `Qwen = 40 %`; the fraction a model holds is what its
producers did, read off the chain by whoever asks. A network that in five years holds a hundred
models holds a hundred rows and one `W`.

## 20. Simulation

`scripts/palw-share-sim.py` — a fluid model with integer block semantics where the class DAA reads
counts; the shipped arithmetic for A (§1), the interim seating and Upgrade C for A′, D as §7, E and
B as §6. `python3 scripts/palw-share-sim.py --supply-x 1` prints the table below (testnet-11's
escrow, eight ready seats, the profile globals, `rate_max = 9 MSK / G`); `--supply-x 10` the
capacity-bound regime; `--no-hold` the waste without the producer's pre-check. Columns: MSK per
G MAC-eq to the producer, the spread of that across models, block and Final-work shares, claims a
span, the ticket, supply utilisation, the block-count oscillation (CV over ten epochs), the HELD
fraction, the floor's block share, blocks a span, issuance an epoch, epochs to converge.

Headline rows at supply = the panel's capacity (the full table is the script's output):

| scenario | design | model | MSK/G-CCU | ρ spread | Final work share | blocks a span | note |
|---|---|---|---|---|---|---|---|
| Q25/Q36 = 50/50 | A | Q25 / Q36 | 9.9e-4 / 4.0e-3 | 4.02 | 0.50 / 0.50 | 0.11 | both at the floor's price, PROBATION for 120 epochs |
| | A′ | Q25 / Q36 | 7.20 / 7.20 | 1.00 | 0.50 / 0.50 | **0.49** | fair while uncapped; the cadence collapsed (§3.5) |
| | **D** | Q25 / Q36 | 7.20 / 7.20 | 1.00 | 0.50 / 0.50 | 5.00 | |
| Q25/Q36/Kimi = 20/30/50 | A | ×3 | 9.9e-4 / 4.0e-3 / 2.8e-4 | 14.2 | 0.2 / 0.3 / 0.5 | 0.11 | |
| | A′ | ×3 | 6.26 / 7.20 / 7.20 | 1.15 | | 0.27 | |
| | **D** | ×3 | 7.20 ×3 | 1.00 | 0.2 / 0.3 / 0.5 | 5.00 | claims a span 0.015 / 0.022 / 0.037 |
| new model at epoch 60 | A′ | Kimi | 0.474 | 15.2 | 0.20 | 0.57 | PROBATION at the end |
| | **D** | Kimi | 7.20 | 1.00 | 0.20 | 5.00 | at its share the epoch it is admitted |
| Q36 as 100 classes | A′ | Q25 / Q36#* | 0.18 / 1.27 | 7.04 | | 0.12 | both in PROBATION |
| | **D** | Q25 / Q36#* | 7.20 / 7.20 | 1.00 | 0.50 / 0.50 | 5.00 | identical to the one-class row |
| 1000 tiny classes, Q25's compute | A | S#* vs Q25 | 0.168 vs 9.9e-4 | **169** | | 0.11 | the cheap class paid 170 × per CCU |
| | **D** | S#* vs Q25 | 7.20 / 7.20 | 1.00 | 0.40 / 0.40 | 5.00 | |
| Q36 supply × 10 at 60 | A′ | Q25 / Q36 | 7.20 / 2.97 | 2.42 | 0.09 / 0.91 | 0.34 | Q36 through HELD → PROBATION |
| | **D** | Q25 / Q36 | 7.20 / 7.20 | 1.00 | 0.09 / 0.91 | 5.00 | issuance 4,032 → 20,000 an epoch |
| Q25 panel outage 60–70 | **D** | Q25 | 7.20 | 1.00 | 0.50 | 5.00 | claims refused during, back at once |
| supply × 0.1 at 60 | **D** | both | 7.20 | 1.00 | 0.50 / 0.50 | 5.00 | issuance 4,032 → 403; `W` at `W₀` |

§20.1 (appended when the run completes) gives the ten-times-capacity table: the regime where the
panel binds, where D's producers hold at the budget and E under-pays the capacity-bound class.

## 21. Number hygiene

Written 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement` as 0137, the next free
number in the index. A concurrent claimant renumbers the later writer.
