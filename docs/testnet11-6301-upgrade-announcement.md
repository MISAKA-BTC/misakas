# Testnet-11: the DAA 6,301 PALW upgrade, and Verification V2 at 6,400

> **The heights moved on 2026-09-18, and this is the second move.** A pre-rollout measurement of the
> live chain found that testnet-11 has **no `bits`-priced producer** — of the last 60 selected-chain
> blocks, 60 are algo 6 and none is the hash anchor ADR-0138 had assumed. Holding the release while
> that was fixed took the tip past the old 6,000 boundary, so every height was re-pinned above it.
> The rule that closed it is ADR-0138 §3c: the heartbeat lane now yields only to a parent that
> actually advances the clock, so a chain with no priced lane keeps its DAA at the target block time.
> [The clock audit §2](palw-daa-clock-audit-2026-09-18.md) has the measurement and
> [the release report](palw-release-6001-verdict-2026-09-18.md) §7 has the fix.

**Fingerprint of the release: `7920f7b233695172959046ce2d7c18cc58729753a3cbc90d0b4ba27c8ec3c30f`.**
Every node on testnet-11 must run this build **before DAA 6,300**. The build currently deployed prints
`ae1d6162…` and is refused from 6,300.

```bash
kaspad --testnet --netsuffix=11   # the first log lines print the fingerprint and the fence schedule
```

## The two heights, and why they are two

**DAA 6,300 — the compatibility boundary.** The held regime (ADR-0118/0119/0121), the one-move court
(ADR-0100) and the 2026-09-11 audit's deep fixes fire here. No rule of the PALW upgrade below fires at
6,300: it exists so the fleet is on one binary before anything economic changes.

**DAA 6,301 — one PALW upgrade day.** Every consensus change of the upgrade fires together, and no part
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
| the single lottery — an attempt block's Layer-0 digest is no longer compared to `bits`, the class ticket is the whole lottery | 0132 S |
| the short challenge window: a claim licensed past 6,301 is challengeable for 120 DAA, not 1,200 | 0132 §7.6 |
| the anchor clock: a block advances the DAA score only if `bits` priced it; a heartbeat stands in where a mergeset carries no priced block at all, and the lane runs at the target block time whenever its parent is not pacing the clock. The attempt and receipt lanes stop pacing the DAA-counted windows | 0138 |
| the execution lane's gas: one 3 M budget per permitted round a chain block merges, under a 390 M ceiling (O13 decided) | 0139 |

**DAA 6,400 — Verification V2 (S1, segmented replay) and readiness V2.** A receipt may name the segments of a job it
attests; a claim licenses when the panel's quorum holds **and** every segment of the anchor's cut is
attested twice. A whole-job receipt is a full attestation, so this changes no licence until seats file
partial masks — a fleet adopts segment replay seat by seat. ADR-0133 §11.1.

The same day, a seat's possession proof becomes a **multiproof over the whole artifact**: the
challenge draws sixteen leaves from the full inventory for each (class, bond, span), the proof opens
them all at once, and its signature covers the bytes it opened. The one-leaf proof of 6,301 is
refused from 6,400, and a seat re-proves every few spans instead of every thirty. A seat that holds
the artifact needs no change beyond running this build; a seat that was answering with one window
will stop counting. ADR-0133 §11.2.

**DAA 6,501 — the compute overlay retires** (ADR-0134). **DAA 6,900 — the market's least seed** (ADR-0120).

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
2. Restart every node — producers, panel seats, pool slots — before DAA 6,300. No datadir move, no resync.
3. After 6,301, read the registry and the economics:
   ```bash
   misaka --network testnet-11 palw registry
   misaka --network testnet-11 palw economics
   ```
   A class's row shows its lifecycle state, the seats ready for it, its `CCU/W` in permille and the panel's
   room. A class with no row draws nothing: register it and prove possession (`--palw-register-class`, then
   the node's own readiness proofs).

## What changes for a model class

Past 6,301 a class's *share* is a result, not an input. The lottery prices a block against the network's one
work target, the registry's readiness and capacity rules decide whether a class admits claims, and the payout
pays the compute the class actually ran. A class that registers on a running chain needs: its graph, its
artifact root, its byte count (`ClassManifestV2`, which the CLI's extension route carries with the
registration), and seats that hold the artifact and prove it.
