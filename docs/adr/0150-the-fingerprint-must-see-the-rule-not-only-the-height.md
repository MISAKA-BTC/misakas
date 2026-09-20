# ADR-0150 — The fingerprint must see the RULE, not only the height

**Status:** ACCEPTED, 2026-09-20. Implemented behind no fence: the manifest is a description of the
build, and adding it re-pins every preset's fingerprint exactly once, on testnet, before mainnet
genesis freezes the format.

> Two builds whose `consensus_params_id` is the same must return the same VALID/INVALID verdict for
> every block. Today they can differ, and the identity cannot say so.

## 1. The hole, as it was walked into

`consensus_params_id` hashes `Params`. For a fenced rule it hashes the fence's **height** and
nothing else — the code says so at each arm:

```rust
// ADR-0133 §11.2 readiness V2: the height only, Some-only.
if let Some(activation) = palw_readiness_v2 {
    h.write(b"palw_readiness_v2");
    h.write(activation.daa_score().to_le_bytes());
}
```

On 2026-09-20 the possession rule that fence turns on changed (ADR-0133 §11.2 as amended: the V2
challenge opens the prefix of its draw that one carrier's budget buys, because all sixteen of its
leaves serialized to 184,037 bytes for the shipped class and no transaction carries that). The
height did not move, no field of `Params` moved, and so:

```
old build fingerprint = ABC          new build fingerprint = ABC
       the same block:  old → VALID          new → INVALID
```

Nothing in the handshake distinguishes them. They peer, agree about every block until the height,
and split there — a partition between nodes that each believe they are on the same network, with no
event anywhere that says which rule anybody is running. This tree already has a name for that shape
(the 2026-09-17 note: *a cleanup that removes history rules without moving the fingerprint is a
silent fork*), and this is the same shape arrived at from the other direction: not by deleting a
rule, but by redefining one the fingerprint only knows by its address.

The readiness change is safe to ship **this time** because `palw_readiness_v2` has never been in
force anywhere — `None` on every shipped preset, scheduled on testnet-11 at a height that chain has
not reached — so the deployment rule is operational: every node carries the new build before that
height. That is a true mitigation for one instance and no answer at all to the class.

## 1.5 The goal, stated so it can be argued with

> **Same fingerprint ⇒ same consensus-relevant semantics.**

Not "same source", not "same binary": same *meaning* for every question a node answers about a
block. That sentence is what makes the rest of this ADR checkable, and it names the subsystems it
covers — header validity, the state transition, fork choice and weight, the DAA, reward accounting,
PALW admission, FreePrompt accounting, pruning and IBD interpretation, and the activation schedule.
A ruleset outside that list is a place the goal does not hold, which is why §2.1 lists them all.

### The ways a fingerprint fails, and what this does about each

The realistic danger is not a broken hash. At 512 bits the collision question is not the one to
spend attention on; **omission** is.

| failure | what it looks like | what this ADR does |
|---|---|---|
| **coverage** — a rule that decides validity is not hashed | the 2026-09-20 readiness change: same height, different verdict, same id | every subsystem is a manifest entry (§2.1); a rule not listed cannot be versioned, so the list is asserted by a test |
| **canonicalisation** — one meaning, two byte strings (or two meanings, one) | map iteration order, optional fields, integer widths | the manifest is a const array in its own order, every field length-prefixed; `the_manifest_encoding_is_canonical_and_unambiguous` |
| **domain separation** — `A=12,B=3` and `A=1,B=23` share a preimage | ambiguous concatenation | the digest leads with a format tag and the entry count, and each name is length-prefixed; same test |
| **rollback / downgrade** | a peer negotiates an older fingerprint format | nothing here is negotiated: the manifest is local, read from the build's own constants, and there is no compatibility mode to select. R1's silence is an encoding property, not a mode — it cannot be *asked for* |
| **partial coverage** | headers hashed, state transition or pruning not | §2.1's list is the whole of the consensus surface this tree has, including `pruning_ibd`, which is the quietest one: two builds can reconstruct different states without either rejecting a block |
| **manifest vs implementation** | the build says `READINESS_R2` and the code is R3 | §3.5's stored corpus: a verdict that moves without a revision is a red build, with the message saying which file to change |
| **over-trust** | same fingerprint, different verdict from UB, platform, overflow, endianness | out of scope for a hash and said so: the fingerprint is a *claim about intent*, and the corpus is the only part that observes behaviour. This ADR does not promise more |
| **hash strength** | collision on the id | BLAKE2b-512, keyed and domain-separated; the least likely of these and the only one that is purely cryptographic |

## 2. Decision

**The fingerprint hashes a rule manifest beside the params and the schedule.**

```
ConsensusFingerprint = H(
    network identity,
    consensus params,
    activation schedule,
    rule manifest:  header_validity, state_transition, fork_choice, pruning_ibd, daa,
                    palw_clock, palw_accounting, palw_freeprompt, palw_admission,
                    palw_readiness, palw_payout, palw_court, palw_verification,
                    palw_work_target, palw_independence
)
```

Each entry is a **semantic revision** of one ruleset: a small integer a developer raises when the
meaning of VALID changes inside that ruleset, whatever height it takes effect at.
`palw_readiness_v2` with `activation = 7,200, semantics = READINESS_R2` is a different fingerprint
from the same height at `READINESS_R1`, which is the property §1 lacked.

**Not the binary and not the commit.** A build hash would make a log line, an optimisation or a
compiler bump a consensus incompatibility, which teaches operators to ignore the field — the
failure mode this decision exists to avoid. The manifest is written by hand, in one file, and the
cost of that is the point: a change that alters validity must be accompanied by a developer saying
so.

### 2.1 The subsystems the manifest covers

```
header_validity   state_transition   fork_choice   pruning_ibd   daa
palw_clock   palw_accounting   palw_freeprompt   palw_admission   palw_readiness
palw_payout   palw_court   palw_verification   palw_work_target   palw_independence
```

The activation schedule is the tenth item of §1.5's list and is already hashed as heights, by the
fence arms and by `consensus_schedule_id`; the manifest does not repeat it.

**R1 is silent.** A ruleset at revision 1 — "the rule as this tree first wrote it" — contributes no
bytes, so adding entries does not move any fingerprint and a build that has changed nothing
fingerprints exactly as the build before the manifest existed. The first bump is the first time a
number moves, and today there is exactly one: `palw_readiness` R2. That is also what let this land
without partitioning a fleet mid-rollout.

### 2.2 Which id each revision enters, and why it is not all of them

This tree already separates three ids, and the manifest follows the same discipline rather than
inventing a fourth:

| id | what it is for | what the manifest adds |
|---|---|---|
| `consensus_identity_id` | the handshake's gate: two nodes that differ here are different networks and do not peer | the revisions of rulesets **in force** (unfenced, or fenced at genesis) |
| `consensus_params_id` | the full fingerprint an operator reads and a release is pinned to | **every** revision |
| `consensus_schedule_id` | reported, never gated, so a mismatch can be named | unchanged |

A ruleset whose fence is scheduled in the future is normalised out of the identity exactly as its
height is (`for_each_fence` rewrites a future height to "not yet"). That is what lets a fleet roll
out a new rule before the height it takes effect at: the builds are one network until the fence
fires, which is the property ADR-0066 SA-4 and audit3 H1 protect and which this must not break.

The consequence is the honest one: **a rollout across a scheduled rule change is still an
operational deadline**, and the node now prints the deadline and the rule tag beside its
fingerprint so the operator can compare two nodes by reading two lines. What the manifest removes
is the case where nobody could have known at all — a rule in force today, changed silently.

## 3. The invariants, pinned as tests

1. **A change to consensus validity moves the fingerprint.** Concretely: the corpus test below, plus
   `a_ruleset_revision_moves_the_params_id`.
2. **Comments, logs, RPC shapes and UI do not move it.** The manifest is hand-written and mentions
   none of them; `the_manifest_is_revisions_and_nothing_else` pins its shape.
3. **An activation height change moves it** — already pinned by
   `every_fence_the_visitor_reaches_moves_the_fingerprint`.
4. **A ruleset revision change moves it**, and moves the IDENTITY only where that ruleset is in
   force: `a_scheduled_rulesets_revision_leaves_the_identity_alone`.
5. **Same fingerprint ⇒ same verdict.** A stored adversarial corpus of consensus objects is replayed
   through the validity path and the verdicts are digested; the digest is pinned beside the
   manifest. A change that moves a verdict fails that test, and the failure says: raise the
   ruleset's revision and re-pin. This is not a proof — a corpus is a sample — but it converts
   "somebody should have noticed" into "the build does not compile green".

## 4. What this does not do

* It does not make an old node validate a new rule. It makes the difference **visible before the
  height** rather than discoverable after it.
* It does not version the wire protocol, the RPC surface or the database layout.
* It does not replace a flag day. A rule that changes what an in-force ruleset accepts still needs
  a height, a drill that crosses it, and a fleet that updates — the manifest is what makes a build
  that skipped any of those refuse to be mistaken for one that did not.
* It is not a substitute for reading the diff. A developer who raises no revision on a validity
  change defeats it; §3.5's corpus is the backstop that makes that a red build rather than a quiet
  one.

## 5. Order of work

1. The manifest, the two ids, the invariant tests, the node's printed line — this ADR's commit.
2. **Testnet-11 carries it first.** Every preset's pinned fingerprint moves once, here, and the
   pins are re-taken in the same commit; the identity of a network with no in-force manifest entry
   does not move, so the rollout is a rollout and not a partition.
3. Before mainnet genesis: freeze the manifest's field set and its encoding. Adding a ruleset after
   that is itself a fingerprint move, which is correct, but the FORMAT should stop changing.

## 6. The first entry

`palw_readiness = R2` records 2026-09-20's change (`9ef0d326`): the V2 possession challenge is
spent in bytes — the prefix of the draw one carrier's budget buys — because sixteen leaves did not
fit a transaction. R1 is the rule as ADR-0133 §11.2 first wrote it. No chain has run either, and
testnet-11 arms the fence at 7,200; the fleet's deadline is that height.
