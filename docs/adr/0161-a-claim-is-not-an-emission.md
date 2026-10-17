# ADR-0161 — A claim is not an emission: multi-claim carriage under a fixed per-DAA reward budget

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

* Status: **Proposed** (rcore/cap-s1, stage 5 of the user's capacity plan). Implemented behind the dormant
  fence F-M1 `palw_capacity_multi_claim` (testnet-12 only); armed nowhere.
* Date: 2026-09-27
* Decides: ADR-0160 v3 D-19 (emission at network ×N), option (b)+(c) together: riders share their block's
  carve, and the network's reward per DAA is the budget the attempt blocks' subsidies already are.
* Related: ADR-0160 §9.2–§9.3 (lane M), ADR-0126 (the carve at the lower DAA), ADR-0152 V-2/V-3 (withheld at
  acceptance, released at maturity, the coinbase identity), ADR-0058 (merged work).

## 1. The user's requirement (binding)

2026-09-27, stage 5 of the capacity plan: **M = multi-claim per block ONLY with an emission redesign** —
a fixed per-DAA reward budget; **claim count ≠ emission count**; show that **emission at ρN equals
emission at ρ1**; write this ADR first, then implement behind its fence.

## 2. What emits today

A testnet-12 block whose own work is an attempt is paid one subsidy `S(daa)` (4,445.62 MSK at launch)
by its coinbase: the worker carve `c = worker_carve(S)` is **withheld** and escrowed on the attempt's
claim (`escrowed_reward = E = c`), released only through `Final → vesting row → maturity`, burned on a
void's forfeit or a conviction; the remainder `S − c` is paid at the coinbase. A skipped own attempt's
carve is withheld anyway and burned (the 2026-09-23 re-audit). Heartbeat and object-only blocks mint no
subsidy. So:

```
minted(DAA d) = Σ_{attempt blocks b with daa(b) = d} S(d)          (1)
```

Two facts follow. **ρ does not appear in (1)**: ρ changes what a bond commits per claim (`⌈w/ρ⌉`,
`⌈E/ρ⌉`) and how many claims it may hold, never how many attempt blocks exist or what each is paid.
**The claim count does, today**: one claim per attempt block, so a network carrying N× the claims per DAA
through attempt blocks mints N× (ADR-0160 D-19: 4.45 M MSK a DAA at 1,000 claims a DAA). The attempt
block rate is the lottery's (each attempt's ticket faces its block's `bits`, adjusted to the network's
target), not the claims'.

## 3. Decision

### D1 — The per-DAA reward budget is the attempt-block subsidy, and nothing a claim does raises it

`B(d) = Σ_{attempt blocks b at d} S(d)` — equation (1) — is the network's reward budget for DAA `d`. It is
fixed by the block lottery's target and the subsidy schedule; neither ρ, nor the number of claims a bond
holds, nor the number of claims a block carries (D2) enters it. **Raising the attempt-block rate target to
carry more claims is refused as a capacity mechanism** (D-19 (a)): it is ×N emission.

### D2 — Riders share their lead's carve (lane M's M-1, under F-M1)

Past F-M1 the bond whose attempt was accepted as a claim (the **lead**) may attach up to `n_max = 64`
further attempts of its own (the **riders**) by one object, `AttemptRidersV1 { lead, riders }`, while
the lead is still `Provisional`. The object is accepted like any object — by a later chain block — so a
rider's job can be derived from the lead's claim id, which exists only once the lead's block is mined
(a rider inside the lead's own body would have to run before the block's template is fixed: circular).
Each rider is its own claim — its own panel, licence, slot, share, room and audit — and none is paid a
subsidy:

```
c        = the lead's escrow as accepted (its block's whole worker carve)
n        = the riders the object carries (1 ≤ n ≤ 64)
E_i      = ⌊c / (1 + n)⌋          each rider's escrow
E_lead   = c − n·E_i  (≥ E_i)      the lead keeps the remainder: Σ = c, no dust
```

The lead's block's coinbase has already withheld `c` (unchanged); the riders redistribute it. A rider
records the lead's carrying block as its `accepted_block`, so the coinbase identity (T03: every withheld
sompi is one claim's escrow, a row, or burned) keeps holding per block. A rider the fold refuses on live
state (its ceiling, lane S, lane N, F-R) is skipped and its share stays with the lead.

**Riders are weightless and floor-only (v1).** A rider carries `pwu = 1`: its claim adds no fork-choice
weight (weight is the lottery's, and a rider won none — the lead's block did), and its `G` is its escrow.
A rider is a floor attempt (the credit, and the CPU audit, are the floor's; D-18). Its job anchor is
`H("misaka-palw/rider-anchor/v1" ‖ lead claim id ‖ i)`; its execution key is the execution commitment
under that anchor (B-4 dedup across the chain).

### D3 — Claim count ≠ emission count

Per block: `Σ_i E_i ≤ c` and `minted = S`, for any `n`. Per DAA: (1) is unchanged, since riders add no
blocks. A claim's reward is its share of its block's carve; a producer that carries 64 riders is paid
what one attempt block is paid, split 65 ways.

## 4. Proof: emission at ρN equals emission at ρ1

Fix a sequence of attempt blocks (the lottery's; ρ and riders do not change it — §2, D1). For any block
`b` at DAA `d`:

* **minted**: `S(d)` at every ρ, with or without riders (the coinbase's subsidy is a function of `d` only).
* **withheld**: `c` with or without riders (a claim escrows it, or a skip burns it); riders only split
  the lead's escrow of it (D2: `E_lead + n·E_i = c`).
* **paid at the coinbase**: `S(d) − c` in every case.
* **released later**: at most `E_lead + Σ_i E_i = c` — each through its own `Final`, vesting and maturity;
  the rest (voided, convicted) burned.

Summing over the blocks of `d`: `minted_ρN(d) = minted_ρ1(d) = B(d)` exactly, and `released_ρN(d) ≤ B(d)`.
ρ enters none of the four lines (it prices collateral: the commitment `m_c = ⌈E_i/ρ⌉` and the reservation
`⌈w/ρ⌉`); the rider count enters only the split of `c`. ∎

The test `palw_capacity_stage5_multi_claim::emission_at_rho_n_equals_emission_at_rho_1` folds the same
block sequence at ρ 1 and at ρ 10 / 25 / 100, without and with riders, and checks the minted, withheld and
escrowed sums per DAA.

## 5. Consequences

* **Per-claim reward falls with the rider count** (`E/(1+n)`); the credited commitment falls with it
  (`⌈E_i/ρ⌉`), and so does `G = w + E_i` — a rider's seat locks and conviction `L` are priced on its own
  (smaller) escrow and its own weight.
* **Fork weight**: none from riders (`pwu = 1`); the block's weight is its lead's, under J-1's cap.
* **A rider needs a live lead**: the object is skipped whole unless the lead is a `Provisional` attempt
  claim of the riders' bond with its escrow unsplit (one riders object per lead).
* **Queues** (ADR-0160 §6–§7): each rider takes an issuance slot, a network unit and a class-room unit
  like any attempt; lane S's bucket and lane N's share bound riders exactly as attempt blocks.

## 6. What this ADR does not decide

* The node's assembly of riders (which attempts a producer carries) — node policy, a later lane.
* M-0a / M-0b (the O(Δ) fold and the incremental state commitment): performance prerequisites before any
  public step carries riders at scale (ADR-0160 §11.6's acceptance: ≤ 1 s fold, ≤ 100 ms root at N4).
* The subsidy schedule itself.

## 7. Fence

F-M1 `palw_capacity_multi_claim`: t12 only, `None` on every shipped preset, Some-only hashed with the
`never()` collapse, on the capacity list; prerequisites at or below it: F-W, F-S, F-N, F-B, F-Q (ADR-0160
§10.1; F-M0 once it exists). Below it an `AttemptRidersV1` object refuses its block; a malformed one
(no rider, more than 64, a duplicate, a bad signature or shape, a rider that is not a floor attempt of
pwu 1 by the lead's bond) refuses its block at any height.
