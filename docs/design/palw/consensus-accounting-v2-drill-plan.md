# Consensus accounting v2 — drill plan (ADR-0172 §8; **not run**)

Status: PLAN, 2026-10-04. The fence `palw_accounting_v2` is dormant on every preset and in no release list. This plan is executed by the lead **after** the 5,300 combined drill ends
(`lanes/DRILL.lock`), one drill at a time, on the binary that would ship, on a salted testnet-12 chain (`--palw-drill-genesis-salt`), never on testnet-12 itself. This lane runs nothing.
The script [`scripts/misaka-palw-accounting-v2-drill.sh`](../../../scripts/misaka-palw-accounting-v2-drill.sh) prints this plan against the lead's environment (`dry`) and reads evidence after the fact (`evidence`); it starts nothing.

## Layout

Four nodes at most, tiny classes, `--ram-scale` (the drill's own nodes starve the panel it tests). `n0`–`n2` are archival and run the ARMED build; `n3` is a late joiner started from an empty datadir after the
fence (IBD); an `old` relay on the pre-fence release (`rcore/int-12` as shipped) keeps the mutual-rejection check. Operators: the first four genesis cards (lane A), `n0` and `n1` hold bonds (`--palw-producer-key`,
`--palw-producer-bond`) and run the FALLBACK miner (`--palw-heartbeat-miner-address`, which now signs the envelope with that bond); `n2` is an external REAL producer with a non-operator bond.
Fences: the 5,300 flag day's list armed low (`--palw-drill-int11-at=A`), the post-launch prerequisites the validator names (F1 same-chain, anchor window, weight cap, lane A, panel seed), and
`--palw-drill-accounting-v2-at=F` with **F a height no other fence uses** (the fork id deduplicates heights) and `F ≥ A + 40` so the crossing happens with REAL work flowing.

## Steps and what proves each (evidence is read from logs and RPC **after** the action, never from configuration)

| # | step | pass criterion |
|---|---|---|
| D1 | **crossing**: heartbeats and a REAL stream through `F` | DAA advances exactly one per slot before, at and after `F` on every node; every chain block at DAA ≥ `F` declares subsidy 0; every chain block of an algo-8 header at DAA ≥ `F` carries an envelope; legacy heartbeats and floors merged across `F` keep their old colouring; one sink, one blue score at one DAA on all nodes |
| D2 | **IBD**: `n3` from an empty datadir after `F` | same tip, sink, blue score, blue work, PALW root and `accounting_v2` ledger as `n0`; the first FALLBACK block is accepted by shape and signature |
| D3 | **pruning proof**: a pruning point past `F` with FALLBACK and E-BLUE headers in level 0 | build, validate and apply succeed; the applier's PALW state at the pruning point equals the archival node's, ledger rows included |
| D4 | **reorg across the fence**: a withheld branch below `F` against the public chain past it, both ways | the branch below `F` loses/wins by the old comparator below and by `safe_weight + fallback_weight` past it; ledger deltas revert exactly (state roots equal after the reorg) |
| D5 | **late REAL after N fallbacks**: N = 1, 5, 13, 14 around the merge-depth edge | on the same chain the REAL is BLUE (N ≤ 13) and RED at 14; on a withheld branch it is classically RED; FALLBACK never reddens a REAL on its own chain |
| D6 | **a 120-round claim**: a REAL claim with a round lane of 120 | DAA +≤ 1 per slot throughout; blue score unmoved by rounds; `safe_weight` ≤ `W_claim` (σ = 0: unchanged by rounds) |
| D7 | **attacks**: a withheld FALLBACK branch absorbing public REAL attempts; an unbonded FALLBACK flood; a junk Exec-lane flood; a duplicate-envelope storm | F1 holds (the branch gains nothing); ticks ≤ 1/slot; unbonded FALLBACKs earn no weight; the envelope signature failure refuses the block at the header stage |
| D8 | **old vs new**: the `old` relay | refused at the handshake from `F` (fork-id mismatch naming `F`); its DAA stops; an unsigned algo-8 block it relays is refused by the armed nodes |
| D9 | **orders**: the same blocks replayed in shuffled orders on a second node and a pruned node | equal verdicts: colour, E verdict, ledger, PALW root |
| D10 | **emission**: ≥ 1,000 claims across ≥ 20 DAAs with voids and a `Final` order shuffle | for every DAA, Σ minted on the PALW routes (tick validator + inclusion + claim payouts vested for that DAA) ≤ `calc_block_subsidy(d)`; a pre-fence claim finalising after `F` pays its escrow as before |

A `Final` through a real panel is the one step the in-tree pipeline test cannot cover (its rig runs no panel); D10 is where it is first seen end to end.

## Metrics (from chain data per window; explorer)

useful-work ratio (C-BLUE REAL ÷ C-BLUE REAL + FALLBACK) and REAL-carried ticks ÷ ticks; REAL BLUE rate against `fallback_count` (must be flat); ticks per slot (max 1); E-BLUE credited ÷ emitted; per-claim weight against `W_claim`; fallback-only stretch
lengths; minted per DAA against the schedule (D10); REAL reds caused by other REALs (the Q7 measure).

## Abort rules

Stop on any divergence of sink / blue score / PALW root between archival nodes; on a DAA step above 1 per slot; on `Minted(d) > B_d`; on a disqualified chain block the template built (template ≠ validation).
Free disk < 15 GB: stop. Take `lanes/DRILL.lock` first; remove it when done.
