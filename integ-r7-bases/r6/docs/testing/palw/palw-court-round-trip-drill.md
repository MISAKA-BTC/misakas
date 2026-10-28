# The court round trip, on a live chain

The audit's acceptance condition for the adjudication layer was not "the court compiles" and not
"a test convicts". It was a **round trip on a live fleet**: a real producer commits one wrong tile,
a third-party challenger actually convicts it, and an honest producer actually clears itself.

The reason it is stated that way is measured, not stylistic. Two defects hid in this layer behind
one-way greens — a close binding that could never match a real claim, and a close that was never
tied to the step under dispute — and both would have survived any number of tests that only ever
ran one direction.

## What had to exist first

Nothing in the tree constructed a `CourtDisclosed` or a `CourtOpened`. The objects existed, the
transitions applied them, the validators checked them, and no code anywhere could produce one. So
before a drill was possible at all:

| piece | what was missing |
|---|---|
| `palw_court_duties_v2` | nobody ever asked the ladder, on behalf of a bond-holding node, whose turn it was |
| `base0_bisect_prefix_state_v1` | `mid_state` was constrained only to differ from the interval's endpoints — enough to stop a responder repeating itself, not enough for two HONEST parties to compute the same answer, so a bisection could not converge |
| `mark_own_material` self-retention | every node got the capture over the wire except the one that ran it |
| retention past `PanelBound` | a court opens on a `ReceiptLicensed` claim, and the tiles were dropped one phase earlier |
| `palw_disputable_claims_v2` + the challenger arm | no opener |
| `palw_court_close_verdict_v2` | a close must announce the verdict the evidence derives to, so a party had to be able to ask before spending a fee |

## The fraud the drill injects, and why that one

`--palw-drill-tamper-leaf=<N>` runs the job, corrupts one lane of step leaf N, and **re-derives the
commitment from the corrupted capture**.

That last part is the whole point. A producer whose roots disagree with its own material is caught
by every seat's `verify_material` before any court opens — injecting THAT would prove nothing about
the court. The fault under test is the other one: an execution that is wrong and a commitment that
is honestly its own. Its capture verifies against its own claim, it licenses normally, and the only
thing in the world that can see it is a node that runs the canonical job itself and gets a different
root.

`the_drill_fault_is_self_consistent_and_only_a_re_execution_finds_it` asserts exactly those three
properties, so a drill that stopped being that fraud would go red rather than quietly prove the
wrong thing.

Both drill flags are refused on mainnet at daemon start, and both log what they are.

## Running it

**Guilty.** One producer lies; one seat re-runs and convicts.

```bash
# producer: commit a corrupted execution in every block
kaspad ... --palw-produce --palw-drill-tamper-leaf=0
# challenger: re-run every licensed claim, dispute what it cannot reproduce
kaspad ... --palw-panel --palw-challenge --palw-fee-outpoint=<txid>:<i>
```

**Innocent.** Everyone honest; one seat disputes anyway and must lose.

```bash
kaspad ... --palw-produce                       # no tamper flag
kaspad ... --palw-panel --palw-drill-challenge-all --palw-fee-outpoint=<txid>:<i>
```

## What to watch, in order

```
[palw-panel] claim <id> committed an execution this node does not reproduce — opening a court
[palw-panel] submitted CourtOpened for court session <sid> round 0
[palw-panel] submitted CourtDisclosed for court session <sid> round 0     ← the responder answers
[palw-panel] submitted CourtVerdictPosted for court session <sid> round 0 ← the challenger judges
   … one pair per rung, the interval halving each time …
[palw-panel] session <sid> closes as ExecutorGuilty on step <N>
```

The last line is the one that matters, and its verdict comes from
`palw_court_close_verdict_v2` — the chain's adjudication of the carried proof, not the submitter's
opinion of it. On the innocent run the same line reads `ChallengerDefeated`.

**A conviction alone is not a pass.** A court that convicts everything and a court that convicts
nothing both produce a clean-looking one-way log. Both directions have to appear.

## Costs, so the numbers are not a surprise

- `--palw-challenge` costs one full inference per licensed claim.
- Opening a court stakes the challenger the claim's own `reserved`, returned when the session ends.
  A drill challenge that is meant to lose still pays for itself.
- The opening rung runs on the session budget rather than the rung clock, because the responder's
  first move is one no software could make until this landed. Later rungs use the tight clock —
  reaching one means the responder answered the first.
