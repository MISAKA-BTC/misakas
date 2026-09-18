# ADR-0143 — An artifact root has one owner on the chain, and competing weights stay permissionless

**Status:** IMPLEMENTED 2026-09-18, **armed on testnet-11 at DAA 6,702** by the operator's decision
of 2026-09-19 to ship it with the 6,700 flag day. Behind its own fence,
`palw_artifact_root_ownership`, dormant on every other preset.

**6,702 and not 6,700**, because the fork id digests the fired heights deduplicated: a fence at a
height the schedule already names is invisible to the handshake, and two builds that disagree about
who owns an artifact from 6,700 would peer as if they agreed. Two DAA of separation is what makes it
a named refusal instead of a silent fork.

**What is drilled and what is not, stated plainly.** The release drill crosses the 6,700/6,701 fences
and its clock gate passed; it does **not** cross this one. The launch runbook's §5c gate asks for a
drill that crosses each armed fence, and this fence is shipping without its own crossing — an
operator decision taken against a deadline (the tip was ~170 DAA from 6,700 at 42 DAA/h). What
carries the risk instead: the crossing block's whole effect is ownership rows and nothing else,
asserted; the migration is deterministic and reverts exactly, asserted; and below the fence every
answer is byte-identical to the chain that has been running. §6 states the one quantity that is
bounded by the state rather than by a constant.

**Builds on:** ADR-0087 (a position is never money settled), ADR-0088 (model lines and versions),
ADR-0091 (the reward buys the pair), ADR-0095 (a position is a membership).

## 1. The defect

Attribution asks "which line does this artifact root belong to", and the answer is decided by
iteration order:

```rust
// model_version_of_root / model_line_of_root, today
for (line_id, line) in self.model_lines.iter().filter(|(_, l)| l.class_id == *class_id) {
    …
    return Some(*line_id);   // ← the FIRST match, in BTreeMap id order
}
```

Two things are wrong and each needs its own fix.

**The chain admits duplicates.** Nothing stops a second line, founded by anyone, from publishing a
version whose root is already another line's — including the class's own founding root.

**The resolution is positional.** With duplicates admitted, which line collects depends on which
`line_id` sorts first in a `BTreeMap`. That is a consensus-visible outcome decided by a hash's byte
order, which is the kind of meaning no rule should carry.

The founding fallback does not save it. It fires only when `model_lines` holds no row *keyed by the
class id at all*, so a third party who founds a line first takes the class's own founding root, and
the class's fallback is disabled from then on.

Live on testnet-11: the Qwen2.5 class's founding root `1a7457f1…` is also carried by a copy line,
and the copy resolves first.

## 2. What this does NOT restrict

ADR-0088's competition stays exactly as it is. **Different weights on the same graph remain
permissionless**, by anyone, without the class registrant's permission:

```
Class A · graph Transformer-X · founding root R1 · registrant Alice

  Line A   R1   ← Alice's founding line, canonical owner of R1
  Line B   R2   ← Bob's fine-tune              ALLOWED
  Line C   R3   ← Carol's independent weights  ALLOWED
  Line D   R1   ← already owned by Line A      REFUSED
```

Only the *exact* root collides. A near-duplicate — a re-quantisation, a conversion, a one-byte
change — is a different root and therefore a different artifact, and the chain does not look inside
it. Judging semantic similarity would put a content opinion inside consensus, which this registry
exists not to have. Near-duplicates are a marketplace and reputation problem, and §8 is where they
are addressed.

## 3. Decision

**D1. An artifact root has one owner, and the chain stores it.** Rooted state gains

```rust
artifact_owners: BTreeMap<Hash64, PalwArtifactOwnerV1 { class_id, line_id, version }>
```

Uniqueness is **global, not per class**. A root is a file of weights; two classes carrying the same
one are the same duplicate the rule exists to refuse, and a per-class index would let the collision
move one level up and keep its ambiguity. The index nevertheless records the class, and every reader
passes the class it is asking about, so a root owned by another class's line answers "not yours"
rather than paying the wrong pair.

**D2. The positional lookups retire.** `model_version_of_root` and `model_line_of_root` stop walking
lines and read the index. Past the fence there is no "first match" to be decided by id order.

The line comes from the index; the **version** is then resolved *inside that one line* — the lowest
version of the owning line carrying the root and in force, and the index's own recorded version when
the line has no rows at all, which is how a class's synthesised founding line answers. A scan inside
a single line is not a walk across lines: no id order can reorder it. **Whether a root is in force is
not moved by this ADR** — `class_roots_in_force` still decides admission, and it decides it exactly
where it did.

**D3. One source for every attribution.** Usage attribution, ADR-0091's buyback, the owner fee and
version lookup all read the index and nothing else. A second way to answer the question is how the
two answers come to differ.

There were **three** call sites, not the two §1 shows. `uncount_claim_usage` — the subtraction a
voided claim makes — carried its own copy of the first-match walk, and one that never filtered by
force while the counting side always did, so on a line that withdrew and republished a root the two
could already land on different rows. Past the fence it asks the counting side's own question with
the claim's own accept height, so the subtraction lands where the addition did.

**D4. A founding root is reserved at registration, atomically.** `ClassRegistered` writes the class
row and the ownership of its founding root in one transition. The fallback that depended on "no line
row exists yet" is deleted with the window it opened: there is no moment when a class's root is
registered and unowned.

**A registration on a root another line already owns is refused**, with the same
`DuplicateArtifactRoot`. The alternative — admitting the class and leaving it unable to own its own
root — would create a class whose own attribution is paid to a stranger, which is the defect itself
rather than a milder form of it. A registrant who meets this has a remedy the chain need not
provide: a different artifact is a different root.

**D5. Every entrance refuses a duplicate.** `ClassRegistered`, `ModelLineFounded` and
`ModelVersionPublished` all reject an owned root with `DuplicateArtifactRoot`, through one helper.
Fixing a single call site leaves the others open, so the rule is written once and every path where a
root enters state is audited against it.

A line republishing a root **it already owns** is not a duplicate and keeps the row it has. The
question the index answers is *which line*, and moving the version on a republication would rewrite
where the claims that already counted point.

**D6. Activation canonicalizes what is already there, deterministically.** At the fence the index is
built from existing state by a total order:

1. a root equal to its class's registered founding root → the **founding line** wins, always;
2. otherwise the **earliest accepted version** wins;
3. a tie → deterministic id order.

Rule 1 is the one that matters: it is what returns testnet-11's Qwen2.5 root to the class that
registered it.

**D7. Legacy duplicate rows stay.** They are historical record and the chain does not rewrite
history. They simply stop being the answer: past activation they attract no usage, no buyback and no
owner fee for a root they do not own.

**D8. Nothing settled before the fence is recomputed.** Past payouts and buybacks are final, the
12,816 MSK already attributed included. The fence changes attribution from the fence, and no earlier
state root moves.

**D9. Its own fence.** `palw_artifact_root_ownership`, independent of the 6,701 bundle, so a failure
in either is one failure domain. It is dormant on every preset here.

## 4. Why the index and not a rule against duplicates alone

Refusing duplicates from the fence would leave the ones already on the chain resolved positionally
for ever, and would leave two ways to answer the question — the index for new roots, the walk for old
ones. D6 canonicalizes the existing rows *into* the index, so after activation there is exactly one
mechanism and one answer.

It also closes a race the announcement itself would otherwise open: between announcing a fence and
reaching it, a squatter could take a root that has no line row yet. D4 removes the window and D6
resolves anything already in it in the founding line's favour.

## 5. What must be proved

* the index answers before and after activation, and the answer never depends on id order;
* the legacy migration is deterministic, and founding-root precedence beats an earlier third-party
  version;
* a duplicate is refused at `ModelLineFounded` **and** at `ModelVersionPublished`;
* a class registration reserves its founding root in the same transition that writes the class;
* a reorg restores the index exactly, and a delta revert equals a fresh walk;
* a snapshot carries the index, and a node that loads one answers as a node that folded the chain;
* usage attribution and buyback attribution both read it, asserted separately;
* a different root on the same class is still accepted, from a bond that is not the registrant's —
  ADR-0088's competition, pinned so this rule cannot quietly eat it.

## 6. What the crossing block costs, and the one thing to measure before arming

The migration runs **before any object of the block that crosses the fence**, in the fold and in the
acceptance filter's pre-object base alike, so admission and the fold cannot disagree about whether
the index exists yet — and a block that crosses the fence and founds a line in the same breath is
judged by the canonicalised index rather than by the empty one it arrived at. Ordered the other way,
a squatter with a fast node would have had one block of warning to take a root the migration was
about to return.

That block writes one delta entry per distinct root, and nothing else. The count is bounded by the
state, not by a constant: at most one root per class plus one per version row, with
`PALW_MODEL_LINES_PER_CLASS_V1` (64) lines a class and `PALW_MODEL_VERSION_HISTORY_V1` (64) versions
kept per line. **The structural ceiling is therefore large enough to be worth measuring rather than
assuming**, which is this ADR's own lesson from the DAA-clock audit.

**The gate before arming is one number: the roots on the target network at the arming height.** It
is read from the chain, it belongs beside the launch runbook's §5c drill gate, and it decides
nothing else — a chain with a few hundred roots crosses in one ordinary block, and a chain with
hundreds of thousands needs the question asked again before a height is picked.

## 7. What this does not decide

Whether the fence is armed, and at what height. That is the operator's, after a drill crosses it.

## 8. The part that is not consensus

Near-duplicate weights are a real problem and this ADR deliberately leaves them to the surface that
can judge them. A model page can say which line is the class's founding one, which are independent
weights, and which is a legacy duplicate row — `Registered by`, `Canonical founding artifact`, `This
line owner`, `Artifact relationship`. **Attribution is never resolved there**: the chain answers it,
and the page displays the chain's answer.
