> **SUPERSEDED 2026-08-26 by [ADR-0053](adr/0053-palw-one-execution-family.md).** There is one
> lane. Family M — the Metal/GGUF lane this document describes as "the one nobody can seat yet" —
> was withdrawn, and the document's own central observation is why: a lane that cannot be seated
> with the hardware that exists, whose share nobody can fill, is not a lane held open. It is a hole
> in the economy that the gates below it were disabled to accommodate. What replaced it is not a
> second lane but a second CLASS in the one family: ADR-0052's `PALW-QWEN36`, which the court can
> re-execute. Kept for the reasoning about panels, seats and operator gating, which stands.

# testnet-11: two lanes, one of which nobody can seat yet

**Decision: the suffix stops climbing here.** testnet-12 and testnet-13 were never published as
networks anyone could join; minting a fresh number for each internal relaunch turns the suffix into
a changelog. testnet-11 is not currently running on any host (measured 2026-08-22), so this
consolidates onto it and it is the number that goes out.

## What the two lanes are

| | **BASE-0** (Family D, deterministic integer) | **CAT-M-0001** (Family M, Metal/GGUF) |
|---|---|---|
| what a block's work is | a derived-weight integer transformer, minted from a seed on every node | a pinned GGUF under a pinned Metal runtime |
| who can produce | any CPU | Apple Silicon only |
| how a seat verifies | rebuild the step leg from the tiles and match the committed root | full re-execution |
| can a court convict in it | yes — the whole step space is adjudicable | yes, by re-execution; no per-step bisection |
| in the genesis | **yes, at 1000‰** | **no** — see below |

## Why Family M is NOT in the genesis, and why that is the design rather than a shortfall

Two measured facts decide this, and neither is a matter of taste.

**A registered class always holds a share.** `ShareBelowGrantFloor` refuses a grant whose worst-case
epoch budget is zero blocks (ADR-0045 Decision 2), so there is no such thing as registering a class
at 0‰ and letting it sit idle. Register it, and it holds cadence.

**A class that holds cadence it cannot fill wedges the chain.** The per-class DAA retarget measures
each lane against the combined census, so a lane whose permille is in the expectation while its
blocks are missing from the total makes the OTHER lane read as an over-producer at every epoch
boundary — target divided until the class lottery refuses every attempt, with no path back. That is
the same wedge the audit recorded at §6 for the receipt lane, and it is why `ATTEMPT_SHARE_PERMILLE`
is 1000 today.

**And Family M cannot be seated with the hardware that exists.** A panel excludes the executor, so
even the thinnest legal panel needs THREE Apple Silicon machines (one producer, two seats), and the
network default of 5 seats needs six. The fleet is four x86 Linux hosts and one Mac.

So a genesis carrying Family M would hold a share nobody can fill, on a network whose only Apple
Silicon machine is a laptop. It would not be "Family M enabled". It would be a chain that stops.

## What ships instead: the lane is open, not present

The genesis declares **`min_class_panel`**, which is the network saying "a class may draw its own,
thinner panel, and this is the floor". Today that value is `(0, 0)`, which
`PalwClassTermsV2::panel_params` reads as *no per-class panel at all* — a fail-closed default I
would otherwise have to keep, because admitting a thin panel is a decision about the network's own
identity and not a registrant's to make.

With the floor declared, `ClassRegistered { admission: Some(..) }` is already an admissible object on
a running chain: `verify_class_admission_v2` checks the graph's coverage, its ladder depth, its cost
bounds and its declared pwu, and the share table is re-divided by the grant rather than invented.
Nothing about that path needs to be built — it needs Macs.

**So both lanes are in the design and exactly one is in the genesis.** That is the honest shape:

- BASE-0 produces from block 1 and holds the whole cadence, so the network is alive on day one.
- Family M's entry is defined, checkable and gated on operators rather than on code. The day a
  second and third Apple Silicon node exist, a registration transaction adds it and the share table
  splits — no re-mint, no flag day.

## The identity, measured

| | |
|---|---|
| network | `testnet-11` |
| genesis hash | `d25a80b9045abb97…` |
| consensus fingerprint | `048e69026e559e67584ded64f1b6279148e3459975ef9d710e029eaaed638ee0` |
| premine | 13B split + **347M community (9 entries)** + one 100 MSK fee float per genesis bond |
| `min_class_panel` | `(2, 2)` |
| BASE-0 share | 1000‰ |

**Two things moved with the suffix and had to be chased.** The per-bond fee floats stayed keyed to
12 through the move, so the genesis briefly funded nine community entries and not one of its own
bonds — a registry whose members cannot pay for a lifecycle transaction can license nothing. And
the M-07 round-trip guard was still checking `TESTNET11_PARAMS`, the legacy algo-4 const, which
shares the suffix but pins a genesis computed without those floats; it now checks the preset
`From<NetworkId>` actually returns.

## What has to be true before Family M carries weight

1. **Three Apple Silicon hosts** (producer + two seats), or two plus a decision to declare
   `min_class_panel = (1, 1)` and accept a one-seat panel, which is a weaker claim about
   independence and should be written down as one.
2. `min_class_panel` in the genesis bundle — it is inside `palw_ruleset_id_v2`, so this is the part
   that cannot be added later without a re-mint. **It is why testnet-11 is minted now rather than
   after the Macs arrive.**
3. A `ClassRegistered` carrying CAT-M-0001's admission, signed by a registered bond.

Item 2 is the whole reason this document exists: everything else about Family M can arrive by
transaction, and that one thing cannot.

## What is NOT claimed

- No Family M block has ever been produced on a public chain.
- The court cannot bisect a Family M execution per step; a dispute there is settled by
  re-execution, which is what `is_court_adjudicable` distinguishes.
- The suffix consolidation is bookkeeping, not a technical property: testnet-11's genesis is new,
  and no chain data from any earlier network carries over.
