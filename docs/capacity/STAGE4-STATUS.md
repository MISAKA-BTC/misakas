# ADR-0160 stage 4 (rcore/cap-s1) — status: gate PASS

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

Same branch and base (`rcore/cap-s1` off `c3dbaee3c`; nothing armed; the t12 pins `dbbc9104…` /
`5de80e64…` / `7c652212…` unmoved). Commit `042053712` (lane N, F-R past F-N, lane S's burst).

## What stage 4 is

The user's plan: **N = fair share**, `allowed_i = min(bond capacity, rate, class room, fair share)`;
property test: **13k × 10 bonds never beats 130k × 1 bond**. Behind its own dormant fence F-N
`palw_capacity_network_room` (prerequisites F-R, F-S, lane A).

## The rule (ADR-0160 v3 §7, with the deviations the property forced)

```
L_net   = min(L_seat, L_carry = H_L·LPB·max(1, ā_op), L_anchor = B_bind·max(1, ā_op)·anchor_delay)
L_seat  = the floor's J-6 pipeline + its bound unlicensed claims + every model class's share room
ā_op    = operator-attempt chain blocks a DAA over 32 DAA (rooted ring)
units   = ⌊C / 13,000 MSK⌋;  Reg = bonds registered (rooted (bond, class) → until, t + H_L at a refusal)
share_a = ⌊L_net·units_a / Σ units over Reg ∪ holders ∪ {b}⌋;  owed¬b = Σ_{a∈Reg, a≠b} max(0, share_a − held_a)
admits(b) ⟺ free ≥ 1 ∧ (free − owed¬b ≥ 1 ∨ b ∈ Reg is the most-owed registered bond)
```

* **Deviation 1 — no own-share path, no `max(share, 1)`.** On the rule itself (a Python replica of the
  capacity test: 30,000 random states, adversarial orders) the ADR's `held_b < max(share_b, 1)` over
  `Reg ∪ {b}` let the pieces beat their whole in 1,457 states, by up to **76 claims**: an unregistered, non-holding piece is invisible to the others'
  shares, so the first piece takes the unowed units and each later piece takes its own share out of the
  registered bonds' reservations. Here holders count in the shares, a bond's own share never takes a
  reserved unit (only the most-owed registered bond does — no unit idles between two owed bonds), and there
  is no slack. The honest share stands: nobody takes a unit a registered bond below its share is owed.
* **Deviation 2 — F-R's per-bond share past F-N is the same rule at the class level.** F-R's `room_cap_v1`
  gives every bond one slack unit and draws the floor's racing headroom one unit a bond: ten pieces take
  ten units past their shares where the whole takes one. Past F-N the class room is divided by the rule
  over the class's holders and the bonds registered for that class; the headroom stays out of the shares,
  first come. F-R's refusals (`FloorRoomExhausted`, `BondClassShareExceeded`) register past F-N.
* **Deviation 3 — `L_anchor` floors `ā_op` at 1** like `L_carry` (else the fence's empty ring, or an
  operator pause, refuses every admission); `L_seat` is on `U`'s basis.
* **Lane S's burst is one DAA's refill** (was `max(4, ⌈uρ/25⌉)`): ten 13k bonds burst 40 where 130k burst
  8 at ρ 10, and the ADR's rate `r` was unreachable (13k at ρ 100: 8 a DAA, not 10). Now 13k at ρ 100:
  200 / burst 10 / 10 a DAA (inside the user's 8–16); a one-claim floor binds only below ρ 10 at 13k.
* Registration costs a refused attempt block (its carve burned); the producer's pre-check mines a refused
  attempt while its bond is not registered for the class (node policy), holds back once it is.

## Gate — PASS (`palw_capacity_stage4_network.rs`, 6 tests; `palw_capacity_network_room_is_t12_only.rs`)

| term of `allowed_i` | split-neutral because | evidence |
|---|---|---|
| bond capacity (`N_bond`, 500‰ ceiling) | linear in collateral | fold gate below |
| rate (lane S: `N_out`, burst, refill) | all linear in `⌊C/6,500⌋` | unit test at ρ 10…1000; fold gate |
| class room (F-R past F-N) | the rule at the class level | rule test with headrooms |
| fair share (lane N) | the rule | rule test |

* **The rule, capacity**: 30,000 random states (levels 1–200, up to three bonds of 1–120 units, held, each
  registered or not, half with a class headroom), the pieces in random order vs their whole alone, for
  13k/130k, 100k/1M, 26k/260k: **the pieces never took more** (28,931 ties).
* **Through the fold** (every capacity fence armed, a 1M competitor, the same state and attempt blocks):
  ρ 10 / 25 / 100 credited: equal by every DAA (200 / 240 / 240 in 20 DAA; lane S and the bond's ceiling
  bind); ρ 10 uncredited with the level at 63 and the competitor registered: equal (8 a piece-set and a
  whole until the registration lapses at `H_L`, then 18).
* **Lane N's behaviour**: a lone 1M bond takes the whole level (63) and registers when refused; a
  registered 500k bond takes the ten units the other's licences free while the other (past its share) is
  refused; the ring counts operator attempt blocks over 32 DAA and reverts/IBDs/restarts root for root.

## What "never beats" does not cover (for the user)

The property holds for **capacity** — what any state lets the pieces take, in any order. Under schedules
with releases (4,000 random runs) the pieces came out ahead of their whole in **48 runs, by at most 13
claims**, and behind in all (711,873 ≤ 714,954): a registered bond's reserved units idle until it asks,
which moves the competitors' admissions in time. (In the Python replica the ADR's rule was ahead in 71 of
4,000 runs, up to 13 claims.) Only a rule with no reservation (first come) is neutral under every schedule, and it protects nobody's
share. Guarded by a test (ahead in ≤ 3 % of runs, never ahead in all).

## Open values (placeholders until measured)

`B_bind` = 100 (V4's bind-cost run), `LPB` 3 / 64, `H_L` = 21, the ring's 32 DAA, `C_min` = 13,000 MSK.

## Where a merge of rcore/int-5 touches this

Delta entries 86–87 and carriage tail `0xBA` (after stage 2's 83–85 / `0xB9` / tag 60): renumber at the
merge if int-5 appended its own (dormant, nothing stored).

## Batteries (stage-4 head)

| crate / target | result |
|---|---|
| `kaspa-consensus-core` lib + all 203 integration binaries | 3,807 passed, 0 failed, 41 ignored |
| `kaspa-consensus` lib | 537 passed, 1 failed, 21 ignored — the failure, `t12_capacity_shadow::s_t5_a8_…`, passes alone (284 s): a processor run whose panel draw depends on its timing (card 0 on B's panel or not); the capacity fences are dormant there |
| `kaspad` lib | 428 passed, 0 failed |
| `kaspa-rpc-core`, `kaspa-grpc-core`, `kaspa-rpc-service`, `misaka-cli`, `misaka-palw-extension` libs | 249 passed, 0 failed |
| `kaspa-testing-integration` (`cargo check --tests`) | compiles |
| clippy (`--tests`, the eight crates) | 1 finding on this branch's lines (a `!is_some_and` → `is_none_or`, fixed), 154 on the release base |
