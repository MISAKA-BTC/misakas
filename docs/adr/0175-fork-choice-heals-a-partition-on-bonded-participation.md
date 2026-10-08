# ADR-0175 — The fork choice heals a partition on bonded participation: one search over every tip, fork-relative economic keys, and participation first past `W_p`

**Status:** DRAFT 2026-10-08 (lane FINX, branch `fin/palw-finality-consistency`) — **for the user's decision; nothing is implemented in a
shipping path.** The candidates exist only in the executable model; the status quo's defects are pinned by pipeline tests that change no
rule. Record: [`docs/design/palw/finality-palw-consistency.md`](../design/palw/finality-palw-consistency.md).

**Builds on / amends (if accepted):** ADR-0042 Decision 9 (one comparator), the strict-win reorg rule
(`palw_reorg_strict_economic_win`, lane rcore/f1-strictwin-tie), the pruning-proof strict-economic commit (rcore/hf-pptake2), ADR-0065
D2 (frontier provenance — its "bonds registered after the fork" set difference is reused), ADR-0160 F-W (licence 1000‰, `Created` 0).

## 0. The sentence this ADR is

**A node's sink is the best tip in one order that every honest node computes alike from the same blocks — bonded participation above
the fork first, then the economic keys counted above the fork, then the incumbent — so an honest partition that heals before the seal
converges on the side more bonds worked on, and no branch wins by blue work, by arrival order or by being one block fresher.**

## 1. Context

Devnet r1 (H1, 2026-10-08): a 41-minute partition healed on the wire and stayed split for good. LIVE-R1 traced it; FINX's question was
whether the GHOSTDAG finality guard and the PALW fork choice are consistent. They are not (record §0, §3):

* **V1** the sink is GHOSTDAG's first acceptable candidate, not the comparator's maximum; a lighter branch is never weighed;
* **V2** a deep all-economic tie keeps the incumbent on both sides — an honest partition three DAA long with nothing economic anywhere
  never heals (MEASURED `finx_p0_a_*`);
* **V3** the finality point advances on the node's own heartbeats (600 blue ≈ 200–300 DAA) while the gate is refusing, and then the gate
  is never asked (MEASURED `finx_p0_d_*`);
* **V4/V5** IBD and relay decide the same pair differently; arrival order decides for fresh nodes (MEASURED `finx_p0_c_*`);
* **V6** (safety) the gate compares a candidate with the *previous* sink while the keys move with time, so a bondless private heartbeat
  branch that carries a public licence first — or whose clock runs a tick ahead across a pre-fork claim's `Final` — reverses a payment
  past the shallow window (mechanism MEASURED `finx_p0_e_*`; MODEL);
* **V7** the relay skips the lighter side's blocks below the merge-depth root.

And a bound (record §2.3): no rule that reads only topology, blue work, hashes and the economic keys can both refuse an attacker's
private branch and heal an economically tied honest partition — to the lighter node they are the same blocks. Healing needs an input a
private branch lacks. Matured work is too slow and, on a private branch, forgeable by a quorum-holding adversary that grinds its panels.
Bonded participation is not: an attempt is signed by its bond over a challenge that binds its parents, cannot be copied across branches,
is header-verifiable, and bonds minted after the fork are already excluded by ADR-0065 D2.

## 2. Options (MODEL, record §5; both applied challenge windows)

| | C1 | C2 | C3 | F | **E** | K |
|---|---|---|---|---|---|---|
| what | V2-max over every tip + relay below the merge root | + 4× blue-work override | portable weight out | hold finality on a refused heavier branch | C1 + C3 + participation first past `W_p` | E + 2/3-participation finality crossing |
| heals a tied partition | no | no | no | no | **yes, bonds active on both sides** | as E |
| heals r1 | yes, onto the 2-node side | no | no | no | **yes, onto the bond majority** | yes |
| heals past the seal | no | no | no | no | no | > 2/3-bond side only |
| V6 stale incumbent | reversed | reversed | holds | reversed | **holds** | holds |
| panel collusion | reversed | reversed | reversed | reversed | **holds** | holds |
| DA time bomb (court void after the seal) | holds | holds | holds | **reversed** | holds | holds |
| Sybil-fed fresh node | holds | captured | captured | captured | **holds** | holds |
| trusts blue work | no | **yes** | no | no | at an even split ≥ n/3 only | as E |

C2 is unsafe. F is a regression (it reopens what finality closes, and anyone can hold a node's finality open with a heavier junk branch).
C1 alone heals only where the economics differ and keeps V6. C3 alone closes V6 but turns every portable case into a split. K changes
finality's meaning; not needed for the first step.

## 3. Decision (PROPOSED)

Under one dormant fence, `Params::palw_fork_choice_participation_v1` (a height), past which:

1. **One search.** The sink search weighs every fully validated tip in the finality point's future — not only those GHOSTDAG pops before
   an extension — and takes the best of the incumbent's own extension and every tip the gate admits, in the order below (C1). The relay
   fetches a branch below the merge-depth root when its header-level participation (point 3) could rank it above the node's sink (V7),
   which also bounds the work a junk branch can cost.
2. **Fork-relative economic keys.** Both sides' `(safe frontier, safe weight, live total)` are counted over claims accepted above the
   fork point (C3): a pre-fork claim's licence, `Final` or void is portable and decides nothing. This closes V6's portable form and r1's
   wedge cause. (The frontier compared as a DAA, not a blue score, is a follow-up: heartbeat density inflates blue scores.)
3. **Participation first, for a fork at least `W_p` DAA under the incumbent.** Compare the number of distinct bonds registered at or
   below the fork that signed an attempt header on each branch above it (stake-weighted where bonds differ); then the economic keys of
   point 2; then, where both sides tie on participation at no fewer than ⌈n/3⌉ bonds, GHOSTDAG's order; else the incumbent. Below `W_p`:
   strict-win as armed today, on point 2's keys. `W_p` is sized between the longest interval an active honest bond goes without an
   attempt and the licence delay (testnet-12: anchor delay 20); the model uses 24.
4. **IBD commits by the same order** (`validate_staging_palw_order` asks point 3's comparison of the staged and the local chain, relative
   to their fork), so the relay and IBD paths agree (V4) and an old datadir rejoins the bond majority.
5. **Finality is unchanged.** `finality_depth` stays `window_challenge / 2`; no freeze. A partition longer than the seal stays a split.

## 4. Consequences

* **Heals** every honest partition shorter than the seal (≈ 200–300 DAA, 7–10 h at 600 blue) in which bonds attempted on both sides —
  on the side more bonds attempted on, `max(0, W_p − D)` DAA after the heal (MODEL: `ok+22` at D = 3, `ok+5` at D = 20, `ok+1` past `W_p`).
  r1 converges on the majority's branch, and B's old datadir commits it.
* **Holds** against every private-branch adversary modelled: tied heartbeat branches, junk headers, V6's timing, panel collusion with
  ground panels, the DA time bomb, Sybil-fed fresh nodes. Its assumption, stated: **the attacker holds fewer bonds than the honest bonds
  active above the fork over `W_p`** — the PALW analogue of an honest majority. An attacker that ties the honest participation at a
  third of the bonds or more is decided by GHOSTDAG (blue work).
* **Residuals, named.** (a) A partition in a heartbeat-only period (no bond attempts on either side) is economically and
  participation-tied and stays split — by §2.3 no unforgeable input exists then; the node-local partition watchdog (LIVE-R1's N2) and a
  resync with `--checkpoint` are the remedy. Making a slot's clock carry a bond signature would remove it; out of scope. (b) A partition
  longer than the seal stays split (as on every Kaspa network); K is the protocol answer if the user wants one. (c) The trust assumption
  is about *active* bonds: a period in which most honest bonds are idle lowers the bar.
* **Cost.** No block-validity change and no state-root change. Per non-extension comparison: the attempt headers above the fork on both
  branches (bounded by the finality depth, header data) and a walk of the PALW deltas from the fork (the walk ADR-0065 D2's provenance
  check already makes). The search validates lighter branches it used to skip — bounded by point 1's header-level pre-filter.
* **Fingerprint.** One fence: scheduling it moves the params id and the schedule id, activation moves the consensus identity id (the
  pattern every fork-choice fence here follows). Every node must cross it together; below it the build is byte-identical.

## 5. What this ADR does not decide

Whether to arm it, at what height, and `W_p`'s value — the user's. Whether testnet-12 adopts K later. The frontier-by-DAA follow-up.
Bonded heartbeats.

## 6. Interim, without a fence (PROPOSED)

* LIVE-R1's N2 watchdog and the explained wedge warning (node-local) — so a wedged node says so and stops participating.
* The operator runbook for a sealed node: resync from an empty datadir **with `--checkpoint=<daa>:<hash>` of the network's chain**, so the
  fresh IBD cannot land on whichever side serves it first (V5).
* Merchants: until point 2 or 3 is armed, the small-value rule ("30 blue score with attempts continuing") does not hold against V6 while
  pre-fork claims have pending licences (≈ 25 DAA after a fork) or, on the short challenge window, pending `Final`s (≈ 146 DAA); a `Final`
  settlement anchor after the payment still protects it.
