# Testnet-11: the DAA 6,701 PALW upgrade, and Verification V2 at 6,800

> **The heights moved again on 2026-09-19, and this is the third move.** The release was held for the
> registry drill; its phase 1 passed — including the new clock gate, which is the reason the drill
> exists — but the tip reached 6,259 while it ran, and at the measured 46 DAA/h the build could not
> have been on every node before 6,700's predecessor. Nothing fired and nothing was at risk: the
> deployed build schedules nothing at those heights. The heights are re-pinned above the tip, which
> is the same answer the two moves below record.
>
> **The heights moved on 2026-09-18, and that was the second move.** A pre-rollout measurement of the
> live chain found that testnet-11 has **no `bits`-priced producer** — of the last 60 selected-chain
> blocks, 60 are algo 6 and none is the hash anchor ADR-0138 had assumed. Holding the release while
> that was fixed took the tip past the old 6,000 boundary, so every height was re-pinned above it.
> The rule that closed it is ADR-0138 §3c: the heartbeat lane now yields only to a parent that
> actually advances the clock, so a chain with no priced lane keeps its DAA at the target block time.
> [The clock audit §2](palw-daa-clock-audit-2026-09-18.md) has the measurement and
> [the release report](palw-release-6001-verdict-2026-09-18.md) §7 has the fix.

**Fingerprint of the release: `b16a94770653183bce23c8c184f58e1326e4d9ba5e1b32d51c4e9511a08638be`.**
Every node on testnet-11 must run this build **before DAA 6,700**. The build currently deployed prints
`ae1d6162…` and is refused from 6,700.

```bash
kaspad --testnet --netsuffix=11   # the first log lines print the fingerprint and the fence schedule
```

## The heights, and why they are separate

**DAA 6,700 — the compatibility boundary.** The held regime (ADR-0118/0119/0121), the one-move court
(ADR-0100) and the 2026-09-11 audit's deep fixes fire here. No rule of the PALW upgrade below fires at
6,700: it exists so the fleet is on one binary before anything economic changes.

**DAA 6,701 — one PALW upgrade day.** Every consensus change of the upgrade fires together, and no part
of it fires without the rest:

| rule | ADR |
|---|---|
| panel economy 80/20, seat exposure 3×, the work price | 0124 |
| the execution lane (a second lane inside the cadence) | 0125 |
| validator overlay 20 %, the PALW escrow carve | 0126 |
| the DNS two-stage BFT gate, `T_leak` | 0128 |
| the operator lottery and 5-DAA spans (M1, M2) | 0130 |
| the permissionless model registry (rows, readiness proofs, the capacity gate) | 0135 Upgrade A |
| the economic payout: `min(escrow, attempted × rate)` at 9 MSK a G MAC-eq, the panel's share | 0132 Upgrade C |
| one work target `W` — a block draws against `CCU / W`; no class share, class DAA, epoch budget or seat price is read | 0137 |
| the short challenge window: a claim licensed past 6,701 is challengeable for 120 DAA, not 1,200 | 0132 §7.6 |
| the execution lane's gas: one 3 M budget per permitted round a chain block merges, under a 390 M ceiling (O13 decided) | 0139 |

**Two rules left this day, and why.** ADR-0132 S's single lottery and ADR-0138's anchor clock are
**not** in the list above. They arm together — one setter, so they can never be spelled at two
heights — and a pre-rollout measurement of the live chain found that arming them would have stopped
the chain's clock. testnet-11 has no `bits`-priced producer: of the last 60 selected-chain blocks,
60 are the model lane and none is a hash anchor. Past the anchor clock only a heartbeat can advance
the DAA score, and the rule the heartbeat was admitted under measured it against a parent every new
block replaces — so a chain producing faster than the interval suppressed the lane entirely. The
registry drill reproduced it: the clock frozen at its own flag day with the miner running and
nothing minted.

The rule that fixes it is [ADR-0142](adr/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md),
which is built and drilled but not armed anywhere. These two follow it on a later day, with their own
fence. Nothing else in the upgrade depends on them: the work target arrives on schedule, because the
dependency runs one way — the lottery needs the work target, never the reverse.

## DAA 6,702 — an artifact root gets one owner (ADR-0143)

Attribution asked "which line does this artifact root belong to" and the answer was **whichever
`line_id` sorted first in a `BTreeMap`** — a consensus-visible outcome decided by a hash's byte
order. Live here: the Qwen2.5 class's own registered founding root is also carried by a copy line,
and the copy resolves first. Two defects, each with its own fix: the chain admitted duplicates at
all, and it resolved them positionally.

From 6,702 the chain keeps one index, `artifact_root -> (class, line, version)`, and it is the only
answer. A class reserves its founding root in the transition that writes the class; `ClassRegistered`,
`ModelLineFounded` and `ModelVersionPublished` all refuse a root another line owns. The crossing
block builds the index from the rows already on the chain: a class's own registered root goes to its
founding line however late it published, otherwise the earliest accepted version wins, and a tie
resolves by `(line, version)`.

**ADR-0088's competition is untouched.** A different root on the same class, from any bond, without
the registrant's permission, stays permissionless — only the *exact* root collides, and the chain
never looks inside an artifact to judge similarity. Near-duplicates are a marketplace question and
the model page is where they are answered.

**Legacy duplicate rows stay** as historical record; they simply stop being the answer, and nothing
settled before 6,702 is recomputed. Past payouts and buybacks are final.

**Why 6,702 and not 6,700.** The fork id digests the fired heights, deduplicated, so a fence at a
height the schedule already names is invisible to the handshake: a build carrying the 6,700 set
without this rule would advertise the identical schedule, the two would peer, and they would
disagree from 6,700 about who owns an artifact. Two DAA of separation is what makes the difference a
named refusal instead of a silent fork. It is one release and one operator decision with the 6,700
day; only the height is its own.

**DAA 6,800 — Verification V2 (S1, segmented replay) and readiness V2.** A receipt may name the segments of a job it
attests; a claim licenses when the panel's quorum holds **and** every segment of the anchor's cut is
attested twice. A whole-job receipt is a full attestation, so this changes no licence until seats file
partial masks — a fleet adopts segment replay seat by seat. ADR-0133 §11.1.

The same day, a seat's possession proof becomes a **multiproof over the whole artifact**: the
challenge draws sixteen leaves from the full inventory for each (class, bond, span), the proof opens
them all at once, and its signature covers the bytes it opened. The one-leaf proof of 6,701 is
refused from 6,800, and a seat re-proves every few spans instead of every thirty. A seat that holds
the artifact needs no change beyond running this build; a seat that was answering with one window
will stop counting. ADR-0133 §11.2.

**DAA 6,901 — the compute overlay retires** (ADR-0134). **DAA 6,900 — the market's least seed** (ADR-0120).

## What the two audits found before this was armed

This bundle was audited twice against the code, not the design, and both reports ship with it:

* [The pre-arming security audit](palw-audit-2026-09-18-6001.md) — two Critical and six High
  findings, every one fixed before arming. The two Criticals: a rooted consensus decision that read
  a node's own storage (two honest nodes could have folded different state roots), and a `pwu` rule
  that still priced off a value the flag day retires (which both blocked every honest model block
  and let a registrant buy fork-choice weight cheaply).
* [The release report](palw-release-6001-verdict-2026-09-18.md) — the gates checked on the frozen
  candidate, the three holes this bundle's own fixes opened and how each was found, what a chain
  block at the 390,000,000 gas ceiling actually costs, and the drill that armed the verdict.
* [The DAA-clock audit](palw-daa-clock-audit-2026-09-18.md) — why ADR-0138 exists, what it closes,
  and what it does not: finality, merge depth, pruning and the DNS attestation epoch are counted in
  blue score, which the model lane still paces, so their wall-clock length is about half what the
  120-second cadence alone would give. None of them is a chain-split risk and finality moves in the
  safe direction; the numbers are in §9 of that report.

## What an operator has to do

1. Build this release and check the fingerprint and the schedule on start:
   `1150, 1900, 2150, 2400, 3500, 4000, 6300, 6301, 6400, 6501, 6900, …`.
2. Restart every node — producers, panel seats, pool slots — before DAA 6,700. No datadir move, no resync.
3. After 6,701, read the registry and the economics:
   ```bash
   misaka --network testnet-11 palw registry
   misaka --network testnet-11 palw economics
   ```
   A class's row shows its lifecycle state, the seats ready for it, its `CCU/W` in permille and the panel's
   room. A class with no row draws nothing: register it and prove possession (`--palw-register-class`, then
   the node's own readiness proofs).

## What changes for a model class

Past 6,701 a class's *share* is a result, not an input. The lottery prices a block against the network's one
work target, the registry's readiness and capacity rules decide whether a class admits claims, and the payout
pays the compute the class actually ran. A class that registers on a running chain needs: its graph, its
artifact root, its byte count (`ClassManifestV2`, which the CLI's extension route carries with the
registration), and seats that hold the artifact and prove it.
