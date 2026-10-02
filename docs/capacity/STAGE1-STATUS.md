# ADR-0160 stage 1 (rcore/cap-s1) — status: gate PASS

> **Status (2026-10-02).** "Nothing armed" is the state of the branch at the time. This stage's fences
> ride the capacity package that testnet-12 arms at DAA 1,700 (`PALW_T12_POST_LAUNCH_FENCE_V3_DAA`; see
> [`FLAG-DAY-1500-INTEGRATION.md`](FLAG-DAY-1500-INTEGRATION.md)). The pins quoted here are the values at
> the time; the current identity is in [`release.json`](../../release.json).

Branch `rcore/cap-s1` off the shipped DAA-750 release `c3dbaee3c` (worktree `~/Downloads/MISAKA-wt-b/wt-cap-s1`,
shared target `wt-int4-target`, `nice -n 15`, `-j 4`). Nothing armed anywhere; t12 as shipped pins
`dbbc9104…` / `5de80e64…` / `7c652212…` (every `*_is_t12_only` pin green).

## What stage 1 is

The user's staged plan (2026-09-26): integrate J-1 (F-W) + F-E + F-L + verify (F-R/F-B) + shadow at **ρ = 1,
no claim-count increase**; gate = a state diff over ρ=1 × {floor, 8k, 2M} × {13k, 100k, 1M} showing only
intended differences, and six invariants as property tests.

Merges (each built and tested): weight `3aa4abec4` → `b44b7900f`, escrow `d6a058249` → `b042b3c68`, liab
`deccda81b` → `70399f19b` (+ decision 1 `f53bdae9c`), verify `20571f86d` → `88288c299`, shadow `0e4654527` →
`d5e0a3a88`. One shallow-tie rule (the release's strict-win, no-DAA-lowering; F-W's copy removed; F-W refused
by `validate` on a build without the check). Capacity fences live in `PALW_T12_CAPACITY_FENCES_V1`, never in
the DAA-750 list.

## Stage-1 rule changes made to meet "no claim-count increase" (all behind dormant fences)

The first state diff (231f568cd) measured three increases at ρ = 1 and two findings; fixed in `05758fb6f`:

| was | fix | why |
|---|---|---|
| F-W's reservation `min(w, R_budget − held)` — 8k at 13k 1→2 claims, 2M at 13k/100k 0→1 | reservation `⌈w/ρ⌉` at the claim's acceptance ρ (F-L's step; 1 where F-L is off) | today's at ρ = 1; ρ is the only capacity knob. Fork power stays bounded by J-1's `W_cap`, not by the reservation |
| F-R's measured room (k = 2, measured speed) + stake share — 8k at 100k/1M 3→15/29 | panel capacity `min(measured, shipped reading of a ρ× panel)`; per-bond share divides `min(c_v2, ρ·⌈c_ship/2⌉)`; op 186 reads the same | today's lone-bond share at ρ = 1, split-neutral (T-2(a) was not: 10×13k took 5 of 8k where 130k took 3) |
| F2: class-blind C7 ceiling clipped 8k's own Final to 1/20 at the genesis class target | ceiling applies to the C7 list (params mirror) only | D-3 caps C7 at 8k's weight, not 8k |
| F1: G's weight term read the capped reservation (8k lock 359→178 MSK/seat) | gone at ρ = 1 with `⌈w/ρ⌉` | **at ρ > 1 G must read `reserved × ρ`** — carried by the stage that first arms ρ > 1 (stage 3) |

The shadow (`palw_capacity_formulas_v1`, `palw_capacity_shadow_v1`) prices the stage-1 reservation
(`palw_capacity_consensus_reservation_s1_v1`); the lane's `R_budget` rows are kept for history.

## Gate (a): the state diff — PASS

`consensus/core/tests/palw_capacity_stage1_state_diff.rs` (shared fixture `capacity_stage1_common.rs`):
accepted/refused counts **equal in all nine cells at every stage** (floor 2/15/156, 8k 1/3/3, 2M 0/0/1); the
only differences, each classed with its ADR section: J-1 staged weight and `W_cap` (v3 §5.2), the 2M Final
under the C7 ceiling (D-3), and at the conviction AG-2/AG-3's whole-bond forfeiture and final freeze
(v3 §5.5). No UNEXPECTED line; FINDINGS empty (F1, F2 fixed above).

## Gate (b): the six invariants — PASS (`palw_capacity_stage1_invariants.rs`, 9 tests)

| invariant | test (families) | result |
|---|---|---|
| HONEST-NO-LOSS | 8 seeded served runs (floor/8k, 3 producers of random size), twins in lockstep: collateral, slashes, freezes, paid + vesting of the claims both twins admit; an unserved claim (BindTimeout / NoCapablePanel) | PASS — equal earnings; no slash/freeze; the armed twin holds only the void's commitment to `voided + h_obl`. *Tightened (v3 §5.8/G6): compared on claims both admit; the armed 8k room admits fewer when several producers share it (1–3 one-sided admissions per 8k run)* |
| J1-CAP | 6 seeded scripts (binds, licences, Finals, convictions), every block, every tip reloaded, 3 forks each | PASS |
| LIABILITY-SURVIVES | licensed claim through a sibling's void, a retirement request (accepted: exit stays shut), a reorg (rewound and refolded input for input) and a restart, to retirement; floor and 8k | PASS — 22 checkpoints each, 6 trial verdicts each took the whole bond |
| NO-FREE-VOID | void ≤ served (floor/8k × 13k/100k); a void keeps the commitment to `voided + h_obl`, stays a conviction target, and an in-hold conviction collects exactly what it collects without the void | PASS. *Tightened to v3 §4.2 Proposition 1* |
| REORG-DETERMINISM | 6 seeded floor tapes (revert-to-base, IBD, 3 restarts, 3 reorgs each) + an 8k tape | PASS |
| SPLIT-NEUTRAL | 10×13k vs 130k and 10×100k vs 1M, floor and 8k: issuance, weight, reward | PASS — floor 20=20 and 150≤156; 8k 3≤3 (shipped split took 5: T-2(a) was not split-neutral) |

**Decision the user may want to see:** at ρ = 1 the split-neutral 8k room is shared by all its producers
(together `⌈c/2⌉` = 3), where today two or more producers together take `c` = 5. No single bond loses
anything, nothing grows, and at ρ ≥ 2 the room is `min(c_v2, ρ·⌈c/2⌉)`; but a stage-1 flag day would cut
multi-producer 8k throughput 5 → 3. (No stage-1 flag day is planned: the first is ρ 10.)

## Decisions (2026-09-26 17:50)

Present in the lanes: D-1 (2 FCW per 13k), D-2 (class-neutral FCW), D-3 (C7 Final cap — now on the C7 list),
D-13 (2M C7 cap 1), decision 4 (reporter reward on the tier debit). Applied here: decision 1 (`DaDefault` is
tier class) and decision 2 (credit priced on the route that actually collects: floor Tier(0) — v3 §5.3 moves
the credit's safety to the audit door, stage 2).

## Floor-retry (rcore/f2-floor-retry, `palw_floor_refusal_retry`) interactions — not touched

A step-4c void (BindTimeout / NoCapablePanel) under F-E starts E-4's hold (`m_c + reserved` for 600 DAA) and
stays convictable (AG-5); the V03(1) re-anchor keeps the claim live so no hold starts — the retry must use the
same "keeps obligation" predicate so a re-anchor cannot shed the obligation; F-W's stage stays
Created/Anchored across a re-anchor; `H_cap` must sit past the 1,300 V2 fences.

## Where a merge of rcore/int-5 (V2 list at DAA 1,300; fp 24e1aec3…) touches this branch

params lists (keep three: V1 750, V2 1,300, capacity dormant); step 4c voids and the retry (above); the lock
life (F-L's AS-1/AS-2 price the lock, the lock-life fences date it — compose; re-run L-T4/L-T4b/v_t5); every
`T12_RELEASE` pin moves to int-5's fp in one re-pin. **Stage 2 appends delta entries 83–85, carriage tail
0xB9 and object tag 60**: if int-5 appends its own, renumber ours at the merge (dormant, nothing stored).

## Batteries (stage-1 head)

| crate / target | result |
|---|---|
| `kaspa-consensus-core` lib + all 199 integration binaries | 3,772 passed, 0 failed, 41 ignored |
| `kaspa-consensus` lib | 538 passed, 0 failed, 21 ignored |
| `kaspad` lib | 426 passed, 0 failed |
| `kaspa-rpc-core`, `kaspa-grpc-core`, `kaspa-rpc-service`, `misaka-cli`, `misaka-palw-extension` libs | 249 passed, 0 failed (the extension learned F-L's by-name arm) |
| `kaspa-testing-integration` (`cargo check --tests`) | compiles |
| clippy (`--tests`, the eight crates) | 0 findings on lines this branch wrote; 154 on the release base (two of them deny-by-default, allowed so the run reaches every crate) |
