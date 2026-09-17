# ADR-0124 — The panel is paid out of the claim's reward, a seat holds exposure, and a claim is paid for the compute it certifies

* Status: PROPOSED and IMPLEMENTED 2026-09-17 on `feat/adr-0124-panel-reward-and-compute-weight`
  (from `main` at `6fdf6ba7`), at the operator's request ("現在 PANEL に報酬がないのと小さいモデルと大きい
  モデルで同じ 1 claim 1 ブロックだと圧倒的に小さいモデルが有利な点を改善する … ADR を作成し実装を行なって";
  the design the operator brought is quoted in §1). Behind two bare fences,
  `Params::palw_panel_economy` (Decisions 1–5) and `Params::palw_work_priced_reward` (Decision 6).
  **testnet-11 schedules both at DAA 7,001** (the operator's flag day of 2026-09-17, with ADR-0125,
  ADR-0126 and ADR-0128; §8); `None` on every other shipped preset. **A mainnet card states both from
  genesis** (Decision 7).
* Builds on: [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 10 (the worker reward is a
  carve of the subsidy, escrowed at the accepting block and named at `Final`, never minted before),
  [0045](0045-palw-class-economy-on-chain.md) Decision 1 (`pwu` has one legal value) and Decision 3
  (the share table is chain state), [0061](0061-zero-seat-genesis-and-right-sized-collateral.md)
  Decision 2 (10,000 MSK per genesis seat), [0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md)
  Decision 4 (`Unavailable` abstains), [0069](0069-e2e-adjudicability-is-the-price-of-weight.md)
  Decision 7 (an uncertified class bears no weight — the classes that set the unit are the ones that
  bear weight), [0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md)
  (the seed is share × pwu per inference), [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)
  (the buyback is a slice of the reward), [0098](0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md)
  Decision 2 (a seat that found a lie files nothing), [0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md)
  Decision 3 (a per-claim table that enters the root only once written — the shape the duty row
  reuses), [0114](0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) §3 (why a
  height and not an edit).
* Amends: [0038](0038-palw-is-the-consensus-work.md) Decision D's inter-class clause — "`pwu`
  magnitude must never be read as a cross-class price" — **for the payout and the payout alone**
  (Decision 6, §2.3): fork choice, the budget and the retarget still read no pwu across a class
  boundary; [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 10 (the carve named at `Final`
  is now split and priced before it is named); [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)
  Decision 1 (the slice is five percent of the escrow AS PRICED, so a lighter class's pair buys
  proportionally); the 2026-09-11 mainnet audit's C-02 (the stake-weighted seat draw is retired
  past the panel-economy fence, Decision 5) and BC-SYBIL (a dissenting seat's stake is its
  reservation, three times the claim's, Decision 3).
* Supersedes nothing. Leaves ADR-0098 Decision 2 in force and names its cost (§6, SA-4).

## 0. The sentence this ADR is

**A seat that judges a claim is paid out of that claim's own reward — a fifth of it, one fixed share
per drawn seat, to every seat whose `Valid` receipt the chain credited before the receipt deadline,
with the unpaid shares going to a reserve and never back to the producer — and holds three times the
claim's exposure on its bond for as long as it judges, which is exactly what it loses for
contradicting its panel; a bond is drawn only while it holds ten producer floors and its free
collateral covers that reservation, and every eligible bond draws one ticket; and a claim of a model
class is paid the fraction of its escrow that its class's canonical inference is of the heaviest
weight-bearing model class's, the rest never minted.** No new issuance: every number is a split of
the escrow the accepting block already withheld. Below the fences nothing changes.

## 1. What the operator asked, in the operator's words

> PALW の検証側が実際に計算・待機・slash リスクを負うなら、Panel seat にも明示的な報酬を出すべきです。ただし
> 新規発行を増やすのではなく、現在の PALW 報酬枠を Producer と Panel で分けるのが一番きれいです。
> まずは 80 : 20 くらいから始めるのが良い。
>
> 報酬資格は、Final certificate に採用されたか ではなく、deadline までに有効な Panel Receipt を提出したか
> にするべきです。… receipt が chain 上で独立に観測可能、または certificate が全 seat の response bitmap を
> commit する必要があります。
>
> Panel pool を「回答人数」で増減させない。… 未使用分は Verifier reserve へ繰越 または burn。Producer へ
> 返してはいけない。
>
> 正しい + deadline 内 → Panel reward。無回答 → reward 0。間違った verdict → reward 0 → slash。
>
> Producer が 1 claim について R = claim.reserved を risk に出すなら、Panel seat について E_panel = kR を
> 実際に reserve します。… 「最低 bond 額」を上げるというより、この claim の Panel になる間、この額を他の
> 仕事に使えない + 悪意が証明されたら slash 可能 とする。こちらなら本当に security になります。
>
> 必要 exposure を満たした eligible bond ↓ 1 operator = 1 ticket に戻す考えもあります。
>
> mainnet では Miner 10000 MSK、panel 100000 MSK で固定して。
>
> 小さいモデルと大きいモデルで同じ 1 claim 1 ブロックだと圧倒的に小さいモデルが有利な点を改善する。
> … MSK / canonical compute を揃えられます。

Two further parts of the operator's design — a Panel share derived from verification cost
(10–30 %) and a Panel reward proportional to a per-model verification cost — are recorded in §9 as
not decided here. The execution-lane roadmap the operator brought in the same message (1 BPS →
10 BPS) is ADR-0125.

## 2. What exists, and where it fails

### 2.1 The seat is not paid, and the chain cannot see who answered

`finalize_claim` names one `PalwPayoutV2` for the producer's bond and nothing for anyone else.
`slash_silent_seats` is a documented no-op, and `a_v2_panel_seat_is_never_paid` pinned the fact —
its own assertion message asks to be updated "if this assertion starts failing because seats are
now paid". So filing a receipt was pure downside: a seat put a ML-DSA-87 signature on chain, ran
four interval replays, waited on a deadline, and risked `claim.reserved` (BC-SYBIL) for nothing.

Worse for a reward: the `ReceiptLicensed` object carries "whatever receipts the assembler held at
one tick" (`palw_v2_receipt_quorum_assemble_impl`), and the tree already says what that means —
*"a funded submitter could also simply omit two competitors, permanently, for free"*. A reward keyed
off the licensing object's receipts is a reward the assembler distributes.

### 2.2 The seat reserves nothing

`reserved_exposure` is moved by the producer's claim, by registrations and by accusers and
challengers; the `PanelBound` fold writes the panel record, the phase and a deadline. A seat's stake
on a claim was therefore a *balance it happened to hold* — the deep fence made the dissent slash
`claim.reserved`, but a bond with `min_collateral` (0.004 MSK on testnet-11) risked at most that,
and the draw's eligibility read the same floor a producer reads.

### 2.3 One claim, one carve, whatever the model

Every attempt block withholds the same carve (62 % of its subsidy: 2,756.28 MSK on testnet-11) and
every `Final` names it whole. ADR-0076 seeds each class's target from `share × pwu_per_inference`
so that, *at the seed*, a heavier class wins more often per forward and the pwu a block costs is
the same for every class at equal share. That equalisation lives in the lottery. Where the lottery
no longer rations — a class at its budget with a target at or near saturation, which is where
ADR-0117's one-forward draw and ADR-0123's release take a class — every forward is a block, and one
block pays the same 2,756 MSK whether the forward was 7,708 leaves or 2.7 million. The operator's
sentence is this regime: "同じ 1 claim 1 ブロックだと圧倒的に小さいモデルが有利".

ADR-0038 Decision D refused a cross-class pwu price because a *hand-set* table is a standing
arbitrage. The refusal is kept everywhere it protected something — weight, the budget, the
retarget. The payout is the one place the flat carve *is* a table: every class priced at 1.
Decision 6 replaces that table with the one number the chain already derives, pins on every claim,
and prices the claim's own exposure on.

## 3. Decisions

**Decision 1 — a `Final` claim's reward is split 80 / 20 between the producer and the panel pool.**
`PALW_PANEL_POOL_PERMILLE_V1 = 200`. The reward `R` is the claim's escrow after Decision 6's price
and after ADR-0091's buyback slice; the pool is `⌊R × 200 / 1000⌋` and the producer is named the
exact rest. Nothing is minted: the accepting block withheld the whole carve (ADR-0042 D10) and this
only decides who is named at `Final`. The number is the operator's starting point, chosen to be
measured against — raise it if seats are scarce, lower it if producers are (§9).

**Decision 2 — a seat is paid one fixed share of the pool for a `Valid` receipt the chain credited
inside the receipt window; what the pool does not pay is the reserve's, never the producer's.**

* *The share is fixed at binding.* `per_seat = ⌊pool / K⌋` with `K` the seats the panel was DRAWN
  with, never the seats that answered: a seat's pay does not rise when a neighbour is silent, so
  no seat has a reason to want another one omitted (`palw_panel_split_v1`).
* *Credit is a chain fact.* Past the fence a bound panel writes a **duty row**,
  `panel_duties[claim] = { seat → 0 }`, one entry per drawn seat. When the licensing object lands,
  every `Valid` receipt it carries from a seat on duty sets that seat's entry to the block's DAA
  (`credit_seat_receipts`). A shard part credits its shard's `Valid` seats the same way.
* *The supplementary door.* A claim already `ReceiptLicensed` accepts a further `ReceiptLicensed`
  object until `bound_daa + window_receipt`, carrying only `Valid` receipts of seats on duty the
  chain has not credited, each signed inside the window by the seat's registered key, no seat twice
  (`validate_supplementary_receipts_v1`; the fold re-derives every structural fact in
  `credit_supplementary_receipts`). It moves no phase, charges nobody and licenses nothing; it only
  credits. A seat the assembler left out — by a tick's timing or on purpose — carries its own
  receipt (the node half, `palw_v2_supplementary_receipt_assemble` and the panel service's own
  loop), and "who answered in time" is a fact the chain observed rather than one the assembler
  chose. This is the operator's "receipt が chain 上で独立に観測可能".
* *`Final` pays the credited.* `finalize_claim` reads the duty row, names the producer
  `R − pool`, names every credited seat `per_seat` at its bond's payout payload, and adds the rest
  — the uncredited seats' shares and the division's dust — to `panel_reserve_sompi`, a state scalar
  that is never a payout in this ruleset (it is supply that was withheld and not named, exactly as a
  voided escrow is) and never the producer's. A producer that could keep an omitted seat's share
  would have a reason to omit it. A void pays nobody, as before.
* *A seat's pay is keyed by its payee and accumulated* (`add_panel_payout`,
  `PALW_STATE_V2_PANEL_PAYOUT_KEY_PREFIX = 0xFE`): one `pending_payouts` row per seat bond however
  many claims it is credited on before the drain reaches it, sorted after the claim rows and before
  the market's. The queue therefore grows by distinct payees, not by seats × claims, and
  `PALW_V2_MAX_PAYOUTS_PER_BLOCK = 8` keeps its latency argument.
* *What is not paid, by name.* An `Unavailable` or `Incapable` receipt (not a verification), a
  silent seat (reward 0 — the operator's rule; nothing is charged, ADR-0065 D4 stands), a dissenting
  seat (0, and Decision 3's slash), and every seat of a claim that voids.

**Decision 3 — a drawn seat reserves three times the claim's exposure for the claim's life, and
that is what it loses.** `PALW_SEAT_EXPOSURE_MULTIPLE_V1 = 3`; `palw_seat_exposure_v1(claim.reserved)`
is added to `reserved_exposure[seat]` at `PanelBound` (`reserve_seat_duties`) and released at
`Final`, at every void and at the receipt-timeout redraw (`release_seat_duties`) — the duty row's
presence is what says the seats hold it, and `assert_internal_consistency_v2` rebuilds the seats'
side of the ledger from the rows. A seat convicted of contradicting its panel's quorum
(`slash_dissenting_seats`) loses exactly `3 × claim.reserved`: **what is reserved is what is
slashable.** Three seats of a five-seat panel at 3× put 9× the claim's exposure behind a corrupt
quorum, 10× with the producer's own — the operator's "Producer + 悪意ある Panel quorum で合計約 10 倍"
without pricing any one seat at ten.

**Decision 4 — a bond is drawn only while it holds ten producer floors and its free collateral
covers the reservation.** `PALW_PANEL_COLLATERAL_MULTIPLE_V1 = 10`; the seat floor is
`palw_panel_collateral_floor_v1(min_collateral_sompi)`, and `palw_seat_has_headroom_v1` requires
`reserved_exposure + registration_exposure + 3 × claim.reserved ≤ collateral × fp_max_exposure_ratio`
— the same ceiling every other reservation on the bond lives under, so the collateral behind a claim
a bond produces cannot double as the collateral behind a claim it judges ("reserved collateral は
同時 job 間で共有不可"). One rule, and the operator's mainnet numbers fall out of it: a card whose
producer floor is 10,000 MSK draws seats from bonds holding 100,000 MSK (§9 records what the card's
producer floor is today). `palw_seatable_operators_v1` — the operator warning — counts against the
same floor, so the warning cannot drift from the draw.

**Decision 5 — past the panel-economy fence every eligible bond draws one ticket.** The deep
fence's stake-weighted sortition (C-02) is retired where the economy is armed
(`PalwPanelDrawPolicyV1`: `weighted && economy.is_none()`). Once a seat's risk is the exposure it
reserves and eligibility already requires the free collateral to cover it, stake no longer needs to
buy probability — it buys the capacity to hold more seats at once, one per claim. The self-
reinforcing loop the operator named (大きい bond → 選出率 → 報酬 → もっと bond) has no rung left.
The per-operator dedup and the executor's three-way exclusion are unchanged.

**Decision 6 — a claim of a model class is paid the fraction of its escrow that its class's
canonical inference is of the heaviest weight-bearing model class's; the rest is never named.**
At `Final`, past `Params::palw_work_priced_reward`,
`priced = ⌊escrow × min(pwu, unit) / unit⌋` (`palw_work_priced_reward_v1`) where `pwu` is the
claim's `palw_exposure_pwu_v1` — one canonical inference of its class under `DerivedV1`, the
claimed pwu under `MaxPerAttempt` — **the same number its exposure is priced on: a claim is paid on
what it can be slashed on** — and `unit` is the largest such value among the `Active`,
weight-bearing (ADR-0069 D7) classes other than the liveness floor at the paying block
(`work_priced_escrow`). The buyback (ADR-0091) is five percent of `priced`; Decision 1's split is
of what remains. `escrow − priced` is not named — never minted, exactly as a voided escrow is — so
the schedule the accepting block withheld against is never exceeded and no class is paid *more*
than today; a class at or above the unit is paid the escrow whole. The liveness floor is not a
model and is not priced: its economics are ADR-0068's (the minimum share), and its blocks are paid
at the schedule as they were. A network with no weight-bearing model class prices nothing.

*Why this number and not the block's derived work.* `claim.pwu` is `expected_attempts(target) ×
pwu_per_inference` and rises with every retarget, so paying on it would pay a class more per block
for competing harder — issuance coupled to difficulty, which no schedule can bound. `pwu_per_inference`
is registered, frozen, verified at genesis, target-independent, and already what `palw_exposure_pwu_v1`
reads for the very reason (`ExposureCeilingExceeded` for succeeding). In the regime §2.3 describes
— one forward, one block — it is exactly the compute the block cost. In the lottery regime it pays
a heavier class more per block by the ratio of forward costs; the per-class DAA then discovers the
price by competition, which is ADR-0038's own closure of monoculture, now working in the direction
of the model that costs more rather than against it.

*Why the unit is derived and not a constant.* A fenced constant would need a new fence every time a
heavier class is certified, and until it moved the heaviest class would be under-paid — the
complaint this ADR answers, reproduced for the next model. The unit follows certification: a class
sets it only once it bears weight, which an attacker's registration cannot do. The cost is stated
in SA-5.

**Decision 7 — a mainnet card states both fences from genesis.** `mainnet_card_base_v1` arms
`palw_panel_economy` and `palw_work_priced_reward` at `ForkActivation::always()`, beside the audit
fences, with no history judged under the other reading. testnet-11 reaches them by a scheduled
height or not at all (§8).

## 4. The numbers (pinned in `palw_panel_economy_v1.rs`)

testnet-11's attempt escrow is 62 % of the 4,445.62 MSK block: **275,628,448,680 sompi**
(2,756.28 MSK). A five-seat panel, no pair on the line:

| | sompi | MSK |
|---|---|---|
| producer (80 %) | 220,502,758,944 | 2,205.03 |
| pool (20 %) | 55,125,689,736 | 551.26 |
| one seat (pool / 5) | 11,025,137,947 | 110.25 |
| three seats credited | 33,075,413,841 | 330.75 |
| reserve (two silent + dust) | 22,050,275,895 | 220.50 |
| all five credited: reserve | 1 | — |

The shipped classes under Decision 6, with `PALW-QWEN36` (2,685,360 pwu per inference) the unit:

| class | pwu per inference | priced escrow | of the carve |
|---|---|---|---|
| `PALW-QWEN36` | 2,685,360 | 275,628,448,680 | 100 % |
| `PALW-QWEN25-A16` (graph-v5) | 1,589,424 | 163,140,313,185 | 59.19 % |
| `PALW-BASE-0` (the floor) | 7,708 | not priced — 275,628,448,680 | 100 % |

(The floor's fraction would be 0.29 %, 791,158,013 sompi; the module pins the number and the fold
does not apply it.) The panel's pool follows the priced escrow, so a lighter class's seats are paid
proportionally to the work they replayed — the operator's "Panel についても MSK / canonical
compute を揃えられます", with the chain's one canonical measure.

## 5. Why a height and not an edit

Every number here is written into the state root: the duty rows, the seats' side of
`reserved_exposure`, the reserve, and the payout rows a `Final` names. Editing the fold in place
would re-fold every claim of every node that syncs from genesis under the new rules, write a
different root for every block since the first panel, and split the network between nodes that
re-folded and nodes that did not. An activation keeps every block below the height judged exactly
as it was (ADR-0114 §3). The tables and the scalar enter the root and the carriage only once
written — the ADR-0111 shape — so below the fence a build carrying this ADR roots, carries and
fingerprints byte-identically to one that does not; `PALW_STATE_V2_VERSION` does not move and no
golden vector does.

## 6. Security — the four principles, checked, and the amendments stated before the build

*A free field is a free draw.* No object gains a field. The duty row is written by the fold from
the accepted draw; the credit is written from receipts the acceptance layer verified against the
seat's registered key; the payout amounts are functions of the claim record and the class table.

*Silence is not a verdict.* A silent seat is paid nothing and charged nothing (ADR-0065 D4 stands).
Its share goes to the reserve, so nobody — not the producer, not the seats that answered — gains
from its silence.

*Weight is what certification buys.* The unit is set only by classes that bear weight; an
uncertified registration cannot move any other class's pay.

*The chain never takes the host's word.* The supplementary door re-verifies the signature and the
window; the fold re-derives the row, the seat, the verdict and the window from its own state, so
the sync walk credits exactly what the live path admitted.

* **SA-1 — omission is now unprofitable and curable.** The producer gains nothing from a seat's
  omission (the share is the reserve's), the assembler gains nothing (its own pay is fixed), and
  the omitted seat carries its own receipt until the deadline. What remains is the fee the seat
  pays to carry it; the node carries it only after the licence has landed without crediting the
  seat, so the honest common case (the assembler held every receipt) costs nothing extra.
* **SA-2 — the reservation is priced at binding and released by every door.** `reserve_seat_duties`
  and `release_seat_duties` are the only writers of a seat's exposure; `Final`, every void and the
  redraw pass through the latter before the phase write drops the row it reads;
  `assert_internal_consistency_v2` refuses a state whose ledger and rows disagree, and refuses a
  duty row on a terminal or absent claim.
* **SA-3 — equivocation is not closed here, and paying a receipt makes it worth closing.** A seat
  that signs `Valid` and `Unavailable` on one claim is refused only within one object
  (`DuplicateSeat`); across objects the phase gate makes the second unusable, and the supplementary
  door takes only `Valid`. So a seat cannot be paid twice and cannot be paid for a verdict it also
  contradicted on chain. What it can still do is sign both off chain and let two assemblers race.
  The court's slash for a refuted verdict is what prices that today; a contradiction certificate
  over two signed receipts is the object that would price it by name (§9).
* **SA-4 — ADR-0098 Decision 2's seat is paid nothing, and that is the price stated.** A seat that
  found a lie files nothing and opens a court; on a claim that then voids nobody is paid, and on a
  claim the court does not void the finder is not credited. The finder is compensated only through
  the court's `claim.reserved`. ADR-0098 recorded "Chain: nothing"; this ADR does not change it and
  does not pretend the finder is the seat this pool rewards.
* **SA-5 — the unit is read at `Final`, not at acceptance.** A class certified between a claim's
  acceptance and its `Final` moves the unit for that claim. The direction is always downward for
  the lighter classes and never above the escrow for any; a claim is never paid more than the
  schedule withheld. Snapshotting the unit at acceptance would need a field on the claim record,
  which is a version bump this ADR deliberately avoids; the residual is named rather than hidden.
* **SA-6 — the stratified (shard) draw does not read the seat economy.** `palw_shard_licensing` is
  `None` everywhere; when it is armed the shard draw needs the same floor and headroom, gated
  behind the shard fence (the C-02 precedent).

## 7. What does not change

The block subsidy and the carve (62 %: the accepting block withholds exactly what it did); the
escrow's ladder (escrowed while immature, forfeit on void); ADR-0091's buyback and the pair; the
panel's size and quorum, the anchor, the exclusions, the dedup, the receipt window and the
redraw; ADR-0065 D4 (`Unavailable` abstains) and the court; the epoch budget, the share table,
the retarget and fork choice (no pwu crosses a class boundary there); the fee-only lanes; the
free-prompt lane (escrow 0, nothing to split or price); every network below the fences.

## 8. Arming it on testnet-11

**Armed 2026-09-17** on `feat/palw-exec-lane-and-validator-retirement` (`d6c46f25`): both fences at
`PALW_RC_FLAG_DAY_7001_FENCE_DAA` = 7,001, the operator's height, chosen at DAA ≈5,773 — one past the
7,000 release's height, which a fence of its own must not share — beside ADR-0125's lane, ADR-0126's
carve and ADR-0128's gate. Schedule 1150/1900/2150/2400/3500/4000/6900/7000/7001/2125000; fingerprint
`4787b92a0e20065aac88f8258b581f415ace9f6093dfab35c345b48135269005` (from `ae1d6162…`), re-pinned the same day to
`ab4e7b9c7e20d14cbadc0874312b8c6dff89ca66ed7e9be3af2f5523dc58b5f2` when ADR-0130's operator lottery and 5-DAA spans
joined the height, and to
`dd805c9f2c4e9db3c0d6ffa2d87fa6ffb4263078ab8b7eb857f8fb11f8aa010c` when ADR-0134 scheduled the compute overlay's
retirement at 7,201 (no build under an earlier pin was deployed); the identity
does not move, so the 7,000 release and this build peer until 7,001 and refuse each other from it
(`the_7001_flag_day_keeps_the_7000_release_until_7001`). Deployment is the operator's.

The procedure this section prescribed, as it was followed (the constant it named became the shared
flag day's): (1) set the constant and arm both in `palw_rc_base_params`; (2) add the
height to `fork_id_gate_fences_v1`'s pinned testnet-11 list — a fence at a height an earlier build
already schedules is invisible to the fork-id gate; (3) re-pin `shipped_presets_have_pinned_fingerprints`'s
testnet-11 value in the same commit (the identity does not move, so builds with and without the
fence peer until the height); (4) update the activation-axis row of `docs/adr/README.md`. Every
node must run a build carrying the fences before the height. Nothing about a seat's exposure can
be retroactive: a panel bound below the height holds none and is paid nothing; only panels bound
past it are under Decisions 2–5.

## 9. What is deliberately not decided

* **A panel share derived from verification cost** (the operator's `clamp(α·C_V / (C_P + α·C_V),
  10 %, 30 %)`) and **a per-model verification CCU**. The chain has one canonical compute measure,
  `pwu_per_inference`, and Decision 6 already scales the pool by it through the priced escrow; a
  separately registered verification cost is the coefficient table ADR-0038 D refuses. If measured
  seat scarcity says 20 % is wrong, the permille is the one number to move — by a later fence.
* **A spender for the reserve.** `panel_reserve_sompi` is auditable and unspendable. A later ADR
  may name a source for it (a top-up when seats are scarce is the obvious candidate); until then it
  is supply that was never minted, recorded.
* **Equivocation by name** (SA-3) and **the stratified draw** (SA-6).

The mainnet card's producer floor IS decided, as a bundle value rather than a consensus rule: a
card's bundle states `PALW_MAINNET_MIN_COLLATERAL_SOMPI` (10,000 MSK) as its `min_collateral_sompi`
(`palw_fp_bundle_with_windows_and_floor_v3`, chosen by network type where the windows are), so
Decision 4 yields the operator's 100,000 MSK seat floor; its genesis bonds already declare
`max(derived, 10,000 MSK)` (ADR-0061's carve), so every bond the card seats meets it. testnet-11
and devnet keep the 0.004 MSK policy floor, and their bundles and fingerprints are byte-identical
(`adr0124_the_panel_economy_and_the_work_price_are_dormant_everywhere_and_stated_on_a_card` pins
both floors). Mainnet has not launched, so its ruleset id may move.

## 10. Implementation record (2026-09-17, `feat/adr-0124-panel-reward-and-compute-weight`)

* `consensus/core/src/palw_panel_economy_v1.rs` (new): the three constants, `palw_panel_split_v1`,
  `palw_work_priced_reward_v1`, `palw_seat_exposure_v1`, `palw_panel_collateral_floor_v1`,
  `palw_seat_has_headroom_v1`, `PalwSeatEconomyV1`; the worked numbers of §4 as tests.
* `palw_state_v2.rs`: `panel_duties` and `panel_reserve_sompi` (rooted and carried only once
  written, tail `0xA6`; delta entries `PanelDuties` (42) and `PanelReserve` (43));
  `reserve_seat_duties` / `release_seat_duties` / `credit_seat_receipts` /
  `credit_supplementary_receipts` / `work_priced_escrow` / `add_panel_payout`; the `PanelBound`,
  `ReceiptLicensed` (whole and by parts), redraw, void and `finalize_claim` arms;
  `slash_dissenting_seats` reads the duty; the consistency check rebuilds the seats' exposure;
  `PalwTransitionExtrasV1::{panel_economy_active, work_priced_reward_active}`;
  `PALW_STATE_V2_PANEL_PAYOUT_KEY_PREFIX`, `palw_panel_payout_key_v1`.
* `palw_panel_v2.rs`: `PalwPanelDrawPolicyV1`, the eligibility predicate's floor and headroom,
  `derive_panel_v2_with_policy`, `validate_panel_bound_v2_with_policy`,
  `validate_receipt_quorum_v2_with_economy`, `validate_supplementary_receipts_v1`,
  `PalwReceiptQuorumV2::Supplementary`.
* `config/params.rs`: the two fences at every site a fence is spelled (the list, the identity
  visitor, the schedule id, the Some-only fingerprint write, `override_params`, the four presets),
  `palw_panel_economy_fence` / `palw_work_priced_reward_fence` / `palw_seat_economy_at`; the card
  arms both and states the 10,000 MSK producer floor (`palw_fp_devnet_v3.rs`:
  `PALW_MAINNET_MIN_COLLATERAL_SOMPI`, `palw_fp_bundle_with_windows_and_floor_v3`); `fork_id_v1.rs`
  and the extension's fence map name both fences.
* `consensus/src`: the processor resolves both at the block's DAA into the extras and the draw
  policy at the anchor; the receipt validator runs with the door; the assembler's match names
  `Supplementary`; `palw_v2_supplementary_receipt_assemble_impl`; the sync walk carries both.
* `kaspad/src/palw_panel.rs`: a seat keeps its own `Valid` receipts until their window closes and
  carries one as a supplementary object once the chain licenses the claim without crediting it.
* Tests: `adr0124_*` in `palw_state_v2.rs` (the split and the reserve, the late seat, the dissent
  slash, the void, the redraw, the price of work with the floor whole, the carriage), in
  `palw_panel_v2.rs` (floor, headroom, one ticket, validate/derive agreement) and in `params.rs`
  (dormant everywhere, scheduled moves the fingerprint and never the identity, the card states
  both); `a_v2_panel_seat_is_never_paid` now pins the below-the-fence half and names the other.

## 12. Corrections

* **§4's class table is the build's fixtures, not testnet-11's registrations** (found 2026-09-17 reading the live
  genesis objects). testnet-11 registers `Qwen/Qwen2.5-1.5B/graph-v5@512` at 6,630,544 pwu per inference (its
  canonical job is 63 + 2 tokens at n_ctx 512) and `Qwen3.6-35B-A3B/graph-v3` at 2,685,360 (7 + 2 tokens at
  n_ctx 8), and `Qwen/Qwen3.8-27B/graph-v3` (registered at DAA 1,165, share 1 ‰) at 9,000,776 is the heaviest
  weight-bearing model class — so past 7,001 the unit is 9,000,776, Qwen2.5 is priced at 73.7 % and Qwen3.6 at
  29.8 %, not QWEN36 at 100 % and QWEN25-A16 at 59.19 %. Two consequences are ADR-0131's: a leaf count prices jobs
  of different sizes and kernels as if a leaf were one unit of compute, and the unit is set by whichever
  weight-bearing class is heaviest — a class registered with a 1 ‰ share lowers every other class's pay.

## 11. Number hygiene

0124 was the next free number on `main` at `6fdf6ba7` (README: "the next free number is 0123", and
0123 is resident). No other branch head carried a `docs/adr/0124-*` file when this was written. The
execution-lane roadmap from the same request is ADR-0125, written on the same branch.
