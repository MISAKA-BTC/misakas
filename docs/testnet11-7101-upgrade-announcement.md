# Testnet-11: the DAA 7,101 PALW upgrade, and Verification V2 at 7,200

> **The heights moved a FOURTH time on 2026-09-19, to 7,100.** The release drill's own devnet runs
> its clock on heartbeats past its anchor-clock fence, so the DAA-denominated challenge window its
> finals phase waits on is hours of wall clock rather than minutes — the estimate that set the third
> move was wrong about that, and the drill could not have finished before 6,700. The heights now sit
> above ADR-0120's 6,900, which both this build and the deployed one schedule: below 6,900 the two
> advertise one schedule and peer, from 6,900 the handshake can tell them apart, and 7,000 — where
> the deployed release fires its held regime and this build does not — is where it refuses.
> **Every node must therefore run this build before DAA 7,000, not 7,100.**
>
> **The heights moved on 2026-09-19, and that was the third move.** The release was held for the
> registry drill; its phase 1 passed — including the new clock gate, which is the reason the drill
> exists — but the tip reached 6,259 while it ran, and at the measured 46 DAA/h the build could not
> have been on every node before 7,100's predecessor. Nothing fired and nothing was at risk: the
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

**Fingerprint of the release: `731e9d3a5be048bfc948c1f5a70e3e0de1e134124903b207abefe0ceaa696ea6`.** (It was `c3a5e91dfc9336b0…` until 2026-09-20, when
ADR-0150 hashed the consensus rule manifest into the fingerprint so that a redefined rule stops looking like the rule it
replaced. The identity is unchanged, so a node on the older build still peers — it forks at the readiness fence, not at the
handshake.)
Every node on testnet-11 must run this build **before DAA 7,000** (see the note above). The build currently deployed prints
`ae1d6162…` and is refused from 7,100.

```bash
kaspad --testnet --netsuffix=11   # the first log lines print the fingerprint and the fence schedule
```

## The heights, and why they are separate

**DAA 7,100 — the compatibility boundary.** The held regime (ADR-0118/0119/0121), the one-move court
(ADR-0100) and the 2026-09-11 audit's deep fixes fire here. No rule of the PALW upgrade below fires at
7,100: it exists so the fleet is on one binary before anything economic changes.

**DAA 7,101 — one PALW upgrade day.** Every consensus change of the upgrade fires together, and no part
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
| the short challenge window: a claim licensed past 7,101 is challengeable for 120 DAA, not 1,200 | 0132 §7.6 |
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

## ADR-0143 is in this build and does nothing

An artifact root having one owner on the chain was to arm at 7,102. **It does not arm.** An
adversarial audit on 2026-09-19 found two defects in the fence as written, both in the index it
installs:

* the index is state-rooted with no cap, no eviction and no TTL, while every neighbouring registry
  table has one. One version publication per row, at a transaction fee, buys a row every node
  re-hashes on every block and every new node downloads with the pruning carriage, for ever;
* uniqueness was written across CLASSES. That is wrong: a class id hashes the shape profile —
  `n_ctx`, the batch sizes, the runtime flags — so one artifact legitimately carries several
  classes, and `@512` and `@2048` are two class ids over one root. The rule refuses the second, and
  lets a stranger reserve a public model's root for one line founding before its owner registers.

The code ships inert: with the fence unset the index is never written, and a state without it is
byte-identical to a state before the field. The remedy is a per-class key and a bound on the rows,
and it will arm on its own day after a drill crosses its own fence.

**DAA 7,200 — Verification V2 (S1, segmented replay) and readiness V2.** A receipt may name the segments of a job it
attests; a claim licenses when the panel's quorum holds **and** every segment of the anchor's cut is
attested twice. A whole-job receipt is a full attestation, so this changes no licence until seats file
partial masks — a fleet adopts segment replay seat by seat. ADR-0133 §11.1.

The same day, a seat's possession proof becomes a **multiproof over the whole artifact**: the
challenge draws sixteen leaves from the full inventory for each (class, bond, span), the proof opens
them all at once, and its signature covers the bytes it opened. The one-leaf proof of 7,101 is
refused from 7,200, and a seat re-proves every few spans instead of every thirty. A seat that holds
the artifact needs no change beyond running this build; a seat that was answering with one window
will stop counting. ADR-0133 §11.2.

**DAA 7,301 — the compute overlay retires** (ADR-0134). **DAA 6,900 — the market's least seed** (ADR-0120).

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
2. Restart every node — producers, panel seats, pool slots — before DAA 7,100. No datadir move, no resync.
3. After 7,101, read the registry and the economics:
   ```bash
   misaka --network testnet-11 palw registry
   misaka --network testnet-11 palw economics
   ```
   A class's row shows its lifecycle state, the seats ready for it, its `CCU/W` in permille and the panel's
   room. A class with no row draws nothing: register it and prove possession (`--palw-register-class`, then
   the node's own readiness proofs).

## What changes for a model class

Past 7,101 a class's *share* is a result, not an input. The lottery prices a block against the network's one
work target, the registry's readiness and capacity rules decide whether a class admits claims, and the payout
pays the compute the class actually ran. A class that registers on a running chain needs: its graph, its
artifact root, its byte count (`ClassManifestV2`, which the CLI's extension route carries with the
registration), and seats that hold the artifact and prove it.
