# ADR-0061: Zero-seat genesis, and collateral sized by arithmetic instead of history

- Status: Accepted; implemented 2026-08-30 (`palw-genesis-10b-cap` branch, same re-mint train
  as ADR-0059/0060)
- Depends on: ADR-0060 (the heartbeat lane is what makes both decisions safe), the
  post-genesis bond carrier ("a stranger can register their own bond"), ADR-0059 (the 10B cap
  the collateral re-size returns money under)
- Supersedes: the born-licensable rule (`PanelCannotBeSeated`) of the RC genesis gate, and the
  0.1B-per-seat genesis collateral both ADR-0059 and ADR-0060 §8 carried forward

## The two decisions

**1. A genesis may seat zero bonds.** `verify_palw_genesis_v2` no longer refuses a registry
smaller than `seat_count + 1` distinct operators — down to and including the empty registry.
The refusal's stated premise was "`BondRegistered` may not ride a transaction … a registry too
small has no later repair", and both halves are dead: bonds register on the running chain as
ordinary transactions, and the heartbeat lane (ADR-0060 D1) produces the blocks such a
registration rides even when no bonded producer exists. The bootstrap of a zero-seat network
is therefore fully permissionless:

```
genesis (0 bonds) → heartbeat blocks (bondless, fee-only)
                  → bond registrations ride them
                  → bonded producers light the PALW lanes
                  → sixth distinct operator arrives → licensing begins
```

Until the sixth operator, claims void at `BindTimeout` and their escrow burns — LOUDLY (the
runtime warns per voided block), which is what distinguishes today's bootstrap phase from the
silent-forever failure the old gate was written against. The gate keeps every other check:
collateral coverage (C-08), the bind-window sustain rule, the catalog root, the class list.

**2. Genesis collateral is 10,000 MSK per seat** (was 0.1B — the old vault denomination, kept
"because it was there"). The binding constraint is the DERIVED requirement —
`palw_v2_collateral_for_claim_lifetime_v1` over the dearest registered class, measured at
**3,223.07 MSK** on the shipped three-class card — and the C-08 gate only demands the output
COVER the declaration. 10,000 is a ~3.1× margin over that structural minimum; the margin
absorbs DAA advancing slower than one per block (parallel production against one bond), while
the derivation itself already covers the whole claim-lifetime exposure horizon, `+1` included.

Consequences of the re-size, under the ADR-0059 cap arithmetic (the main wallet pays for every
carve, so the cap never moves):

| | before | after |
|---|---:|---:|
| collateral per seat | 100,000,000 MSK | 10,000 MSK |
| locked across 6 seats | 600,000,000 MSK | 60,000 MSK |
| t11 main wallet (spendable) | 8,852,999,400 MSK | 9,452,939,400 MSK |
| genesis total | 10B exactly | 10B exactly |

A slash of one seat now burns at most 10,000 MSK of operator money instead of 100M — the
penalty finally matches the protocol's own accounting instead of a historical denomination.

## What deliberately does not move

* **The declared collateral** in every `BondRegistered` (the derived 3,223.07 MSK) — so
  `palw_ruleset_id` is byte-identical. What moves is the genesis UTXO set, its commitment, the
  t11 genesis hash and the t11 fingerprint (`17bdff18…`), all riding the re-mint already in
  progress. No other preset's fingerprint moves.
* **The seating arithmetic.** `derive_panel_v2` still needs `seat_count + 1` distinct
  operators to license; zero-seat changes when they arrive, not how many are needed.
* **The shipped t11 card.** testnet-11 still ships its six seats — zero-seat is a capability
  (the mint tool now assembles any registry size, including empty), exercised by the next
  network that wants it, mainnet included.
* **No version bump.** The gate change moves no consensus bytes on any running network: two
  builds on a seated network behave identically, and an old build meeting a zero-seat genesis
  refuses to assemble it at boot — loud, fail-safe, and impossible to mistake for a fork.

## Why this closes ADR-0060 §8

Both items were listed there as "deliberately not decided", pending exactly the operator
decision this ADR records. With them decided, a genesis is at last only what a genesis must
be: the supply (one 10B main wallet under ADR-0059), the community's allocations, and — where
the operator wants a running start — a registry it could equally have grown on-chain.
