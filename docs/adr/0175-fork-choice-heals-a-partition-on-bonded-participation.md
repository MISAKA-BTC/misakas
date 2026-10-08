# ADR-0175 — The fork choice heals a partition on bonded participation: one search over every tip, economic keys counted over each branch's exclusive past, and participation first past `W_p`

> **INTERNAL (user decision 2026-10-08).** Finding V6 is kept out of every public text — docs, commit messages, explorer text, issues.
> This draft and its record live on the unpushed branch `fin/palw-finality-consistency`. V6 is named here only as "the stale-incumbent
> comparison"; its mechanics are in the internal record, §3.

**Status:** DRAFT 2026-10-08 (lane FINX). **For the user's decision: C3 alone or rule E.** Nothing is implemented in a shipping path.
The candidates exist only in the executable model; the status quo's defects are pinned by pipeline tests that change no rule. User
decisions already taken (2026-10-08): no public advisory; no early dedicated fence; **the fix ships in the single full-activation
release**. Record: [`docs/design/palw/finality-palw-consistency.md`](../design/palw/finality-palw-consistency.md).

**Builds on / amends (if accepted):** ADR-0042 Decision 9 (one comparator), the strict-win reorg rule (`palw_reorg_strict_economic_win`,
lane rcore/f1-strictwin-tie), the pruning-proof strict-economic commit (rcore/hf-pptake2), ADR-0065 D2 (frontier provenance — its "bonds
registered after the fork" set difference; note testnet-12 ships `palw_frontier_provenance: None`, so rule E carries its own copy),
ADR-0160 F-W (licence 1000‰, `Created` 0), ADR-0058 (merged work counts, blue or red).

## 0. The sentence this ADR is

**A node's sink is the best tip in one order that every honest node computes alike from the same blocks — bonded participation in each
branch's exclusive past first, then the economic keys counted over that exclusive past, then the incumbent — so an honest partition that
heals before the seal converges on the side more bonds worked on, and no branch wins by blue work, by arrival order or by being one
block fresher.**

## 1. Context

Devnet r1 (H1, 2026-10-08): a 41-minute partition healed on the wire and stayed split for good. FINX's question: are the GHOSTDAG
finality guard and the PALW fork choice consistent? They are not (record §0, §3; all MEASURED through the pipeline on testnet-12's
rules unless marked):

* **V1** the sink is GHOSTDAG's first acceptable candidate, not the comparator's maximum; a lighter branch is never weighed — not even
  UTXO-validated — though the node holds it and it is the comparator's maximum (`finx_p0_b`);
* **V2** a deep all-economic tie keeps the incumbent on both sides: an honest partition three DAA long with nothing economic anywhere
  never heals, symmetric or two producers against one (`finx_p0_a`);
* **V3** the finality point advances on the node's own heartbeats (600 blue: 154 slots at 3.9 blue a slot, 300 at 2.0 — 5.2–10.2 h)
  while the gate refuses; past the fork the gate is never asked again, even once the competitor strictly wins (`finx_p0_d`);
* **V4** IBD and relay decide the same pair oppositely: an old heavy-side datadir COMMITS the light chain its relay never weighs; an old
  light-side datadir refuses the heavy chain (`finx_p0_c`); **V5** arrival order decides for fresh nodes (`finx_p0_c`);
* **V6** (safety) the stale-incumbent comparison: the gate compares a candidate with the *previous* sink while the economic keys move
  with time; reversals past the shallow window without a bond (`finx_p0_e`, `finx_p0_f`; record §3). On the live chain the DNS BFT
  veto (ADR-0128, armed from DAA 0) bounds its reach to the DNS confirmation lag while the overlay is Active and confirming; RFC-0012's
  proposed retirement removes that bound — **so this fix must be active no later than the DNS retirement**;
* **V7** the relay skips the lighter side's blocks below the merge-depth root (code).

And a bound (record §2.3): no rule that reads only topology, blue work, hashes and the economic keys both refuses an attacker's private
branch and heals an economically tied honest partition — to the lighter node they are the same blocks. Healing needs an input a private
branch lacks: bonded participation is one the chain already has (an attempt is signed by its bond over a challenge that binds its
parents; it cannot be re-signed onto another branch; it is header data).

## 2. Options (MODEL, record §5; both applied challenge windows)

| | C1 | C2 | C3 | F | **E** | K |
|---|---|---|---|---|---|---|
| what | V2-max over every tip + relay below the merge root | + 4× blue-work override | portable weight out | hold finality on a refused heavier branch | C1 + C3 + participation first past `W_p` | E + 2/3-participation finality crossing |
| heals a tied partition | no | no | no | no | **yes, bonds active on both sides** | as E |
| heals r1 | yes, onto the 2-node side | no | no | no | **yes, onto the bond majority** | yes |
| heals past the seal | no | no | no | no | no | > 2/3-bond side only |
| V6 (stale incumbent) | reversed | reversed | **holds** | reversed | **holds** | holds |
| panel collusion | reversed | reversed | reversed | reversed | **holds** | holds |
| DA time bomb (court void after the seal) | holds | holds | holds | **reversed** | holds | holds |
| Sybil-fed fresh node | holds | captured | captured | captured | **holds** | holds |
| trusts blue work | no | **yes** | no | no | at an even split ≥ n/3 only | as E |

C2 is unsafe. F is a regression. C1 alone keeps V6. K changes finality's meaning; not needed now.

## 3. C3 alone against rule E — on V6, both forms, and on what else they do

V6 has two forms (record §3): a *carrier* form (a branch that carries a pending licence of a claim both branches hold, before the
incumbent does) and a *clock* form (a branch whose clock runs one slot ahead crosses the `Final` height of a claim both branches hold
first). Both need a claim **both branches hold** whose economic contribution changes with time.

| | C3 alone | rule E |
|---|---|---|
| V6, carrier form | **closed** — the claim is portable, its licence counts on neither side | **closed** — same keys, and participation outranks them past `W_p` |
| V6, clock form | **closed** — the claim's `Final` is portable | **closed** |
| V6 by merged borrowing: the attacker forks earlier and merges the public attempts made between the fork and the payment, so their claims are "above the fork" on both branches | **closed only if "portable" means "accepted in a block in both tips' past"**; reopened if it means "accepted at or below the fork's blue score" | **closed only with the same definition** for the keys AND for participation — counted over each tip's exclusive past, the merged honest attempts cancel; counted "above the fork", they inflate the attacker's participation |
| panel collusion (a quorum-holding attacker self-licensing its own branch's claims) | **reversed** — those claims are exclusive to its branch, and count | holds — participation first |
| liveness: tied partitions | split (as today) | heals where bonds were active on both sides |
| liveness: a portable licence on the heavier side (today: heals) | **split** — the licence no longer counts, the tie keeps the incumbent: a regression | heals |
| liveness: devnet r1 | split | heals onto the bond majority |
| cost | a walk of both tips' exclusive pasts per non-extension comparison (bounded by the finality depth); the search unchanged | the same walk, plus participation (attempt headers, same walk) and C1's search over every tip and relay below the merge root |
| fingerprint | one fence; no block-validity or state-root change | one fence; no block-validity or state-root change |

**Recommendation (PROPOSED): rule E**, with both definitions taken over each tip's **exclusive past** (blocks in one tip's past and not
the other's) — not "above the fork's blue score". On V6 the two rules are equal once that definition is used; C3 alone leaves panel
collusion open and makes one more class of honest partitions permanent, while rule E closes V6, closes panel collusion, and heals the
partitions P0 is about. Since the fix ships in one full-activation release either way, rule E's larger cost is paid once, behind the same
fence. Choose C3 alone only if the release must carry the smallest possible fork-choice change; then accept that partitions stay as
today or worse, and that panel collusion stays open.

## 4. Decision (PROPOSED: rule E)

Under one dormant fence, `Params::palw_fork_choice_participation_v1` (a height), past which:

1. **One search.** The sink search weighs every fully validated tip in the finality point's future, and takes the best of the
   incumbent's own extension and every tip the gate admits, in the order below. The relay fetches a branch below the merge-depth root
   when its header-level participation (point 3) could rank it above the node's sink, which also bounds the work a junk branch costs.
2. **Exclusive-past economic keys.** Both sides' `(safe frontier, safe weight, live total)` count only claims accepted in blocks of that
   tip's exclusive past (in its past and not in the other tip's). A claim both tips hold — licensed, `Final` or voided on either — decides
   nothing. The frontier compared as a DAA rather than a blue score is a follow-up (heartbeat density inflates blue scores).
3. **Participation first, once the incumbent's exclusive past spans at least `W_p` DAA.** Compare the number of distinct bonds registered
   in both tips' common past that signed an attempt header in each tip's exclusive past (stake-weighted where bonds differ); then point
   2's keys; then, where participation ties at no fewer than ⌈n/3⌉ bonds on each side, GHOSTDAG's order; else the incumbent. Below
   `W_p`: strict-win as armed today, on point 2's keys. `W_p` lies between the longest interval an active honest bond goes without an
   attempt and the licence delay (testnet-12: anchor delay 20); the model uses 24.
4. **IBD commits by the same order** (`validate_staging_palw_order` asks point 3's comparison of the staged and the local chain).
5. **Finality is unchanged.** `finality_depth` stays `window_challenge / 2`; no freeze.

## 5. Consequences

* **Heals** every honest partition shorter than the seal (154–300 slots, 5.2–10.2 h on testnet-12) in which bonds attempted on both
  sides — on the side more bonds attempted on, `max(0, W_p − D)` DAA after the heal (MODEL: +22 at D = 3, +5 at D = 20, +1 past `W_p`).
  r1 converges on the majority's branch, and B's old datadir commits it.
* **Holds** against every adversary modelled: tied heartbeat branches, junk headers, both V6 forms, panel collusion with ground panels,
  the DA time bomb, Sybil-fed fresh nodes. Assumption, stated: **the attacker holds fewer bonds than the honest bonds active in the
  public branch's exclusive past over `W_p`**. An attacker that ties honest participation at a third of the bonds or more is decided by
  GHOSTDAG (blue work).
* **Residuals, named.** (a) A partition in a heartbeat-only period (no bond attempts on either side) stays split — no unforgeable input
  exists then (§2.3); the node-local partition watchdog (LIVE-R1's N2) and a resync with `--checkpoint` are the remedy; bonded slot clocks
  would remove it (out of scope). (b) A partition longer than the seal stays split, as on every Kaspa network; K is the protocol answer if
  ever wanted. (c) The assumption is about *active* bonds.
* **Cost.** No block-validity or state-root change. Per non-extension comparison: the exclusive pasts of both tips (bounded by the
  finality depth), their attempt headers, and the PALW deltas over them. The search validates lighter branches it used to skip — bounded by
  point 1's header-level pre-filter. **Not measured** (GAP): the cost on a node fed many junk branches.
* **Fingerprint.** One fence: scheduling moves the params id and the schedule id; activation moves the consensus identity id. Below it
  the build is byte-identical.

## 6. What this ADR does not decide

C3 or E (the user's). `W_p`'s value. K. The frontier-by-DAA follow-up. Bonded slot clocks.

## 7. Until the release (PROPOSED, node-local, no fence)

* LIVE-R1's N2 watchdog and the explained wedge warning — a wedged node says so and stops participating.
* The operator runbook for a sealed node: resync from an empty datadir **with `--checkpoint=<daa>:<hash>` of the network's chain**, so the
  fresh IBD cannot land on whichever side serves it first (V5).
* Internal operational guidance for the stale-incumbent comparison is in the record, §3 (not for publication).
