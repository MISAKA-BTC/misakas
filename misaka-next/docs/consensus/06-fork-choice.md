# 06 — Fork choice

> Normative. RFC 2119 keywords. Citations `path:line` are to the t12 reference (`rcore/int-3` @
> `a0af3c92`, see `PROVENANCE.md`) and were checked with `grep -n`/`sed`; anything not read is
> marked **[unverified]**. Rule IDs `FORK-R*`, invariant IDs `INV-FORK-*`. The full invariant list
> lives in `09-invariants.md`, the attack catalogue in `10-attack-model.md`.

## 1. Purpose

A node holds many valid chains at once: its own, a competing branch a peer relayed, a branch that
arrived late through IBD, a branch somebody built in private and is now publishing. Fork choice is
the one function that says which of them is *the* chain. Everything else in consensus — which
claims are final, where pruning may cut, what a payment's confirmation depth is — reads the chain
this function selects, so a second opinion anywhere in the node is a fork that needs no attacker.

In Proof-of-LLM the thing being counted is not hashes but **verified LLM work that has matured**.
That makes the question subtler than in proof-of-work: if maturity is measured on a branch's own
clock, a branch that controls its own clock matures its work faster than an honest branch that
shares the real one. This chapter defines a comparator that is (a) one pure function, (b) a strict
total order, (c) blind to every branch's private clock, and (d) independent of which chain the node
happened to hold before. It gets (c) not by comparing branches at some common clock reading — every
variant of that fails, §3.2 — but by measuring maturity in verified work instead of in time.

## 2. Concepts and types

### 2.1 What fork choice decides, and what it does not

Fork choice decides **one thing**: given the finalized anchor and the set of valid candidate tips
that descend from it, which tip is selected. It does not decide validity (a candidate is fully
validated before it reaches fork choice), does not decide finality (chapter 07 reads the selected
chain and moves the anchor), and does not decide the order of blocks inside a DAG mergeset
(`11-block-and-transition.md` BLK-R4). Header-first sync MAY order *downloads* by any heuristic; that ordering
is not fork choice and no consensus decision may read it (FORK-R10).

### 2.2 Clocks and the work clock

Working definitions; chapter 01 owns the precise ones.

| Quantity | Meaning here | Does fork choice read it? |
| --- | --- | --- |
| `LocalDaa` | a branch's own DAA score; advances with every block the branch accepts | **No** (FORK-R8) |
| `SafeDaa` | 01 DAA-R11: `max(ChainFinalizedDaa, LocalDaa of the D_SAFE-th most recent licence)` on the branch | **No** — never compared across branches (below) |
| `ChainFinalizedDaa` | the `LocalDaa` of the branch's own finalized anchor (01 §2.4) | **No** |
| `FinalizedDaa` | the `LocalDaa` of the node's finalized anchor; never reverts on that node | only as the anchor's identity |
| work clock `Ω` | cumulative verified weight on a chain from genesis (voided-after-verification included) | **Yes** — it is the only measure of maturity |
| blue score / blue work / timestamps / wall clock | GHOSTDAG structure and header fields | **No** (FORK-R8, FORK-R9) |

**Why `SafeDaa` is not compared across branches.** Minted from licences (01 DAA-R11), a branch's
`SafeDaa` cannot advance without verified work — the property chapter 01 asks of it. But
*how far* it advances per step is a `LocalDaa` difference, and a branch that runs its `LocalDaa`
fast makes each step larger. A comparison of `SafeDaa` values would still reward the fast clock.
Fork choice therefore compares **weight**, and leaves `SafeDaa` to single-branch rules (refills,
deadlines) in other chapters.

This chapter relies on two properties owned elsewhere:

* **W1 — admission is rate-bounded, and the bound is not bought with clock** (claims chapter).
  (a) A branch accepts at most `ρ·Δ + b` of claim weight in any `Δ` ticks of its own `LocalDaa`,
  with two consensus constants:

  ```text
  ρ = Σ_lanes rate_lane · w_max            (claims per tick at full bucket × the per-claim ceiling)
  b = (D_SAFE + 1) · Σ_lanes B_lane · w_max (initial tokens + at most D_SAFE catch-up refills of ≤ B)
  w_max = max(W_max, CCU_max)               (04 POL-R7: W clamped to [W₀, W_max], CCU ≤ CCU_max)
  ```

  03 §2.6 derives the admission count `rate·Δ + (D_SAFE + 1)·B` and needs `k_final ≥ D_SAFE` (07
  FINAL-R2) so that the `SafeDaa` floor never binds after bootstrap. Without `W_max`, `ρ` would be
  unbounded: a branch whose `W` rose would bury claims with fewer claims and less `LocalDaa`.
  (b) Running `LocalDaa` faster does not let a branch accept more weight per unit of real resource,
  beyond the `SafeDaa` lead of 01 S5 (INV-CLAIM-01). (a) is what the calibration below uses:
  `d_bury` of weight after a claim implies at least `(d_bury − b) / ρ` ticks of that branch's
  `LocalDaa` after it. By INV-DAA-02 that is at least as many wall-clock slots, less the one-slot
  drift lead, whatever the branch's speed.
* **W2 — weight is fixed at verification and frozen at safety** (claims chapter). A claim's `Weight`
  is the derived work its panel verified, and nothing changes it. A void (conviction, default,
  timeout) of an entry that is not yet safe removes it from `live`, while its burial contribution
  stays (§3.3). Once an entry is safe it stays in `safe` and `V`. A later conviction slashes and
  forfeits rights but moves no weight (03 CLAIM-R5, 05 COURT-R10).

### 2.3 Types

```rust
/// The node's finalized anchor (chapter 07, which adds `own_frontier`). Every admissible candidate contains it.
pub struct FinalizedAnchor { pub block: BlockId, pub daa: FinalizedDaa, pub safe_anchor_count: u64 }

/// A claim on a candidate's chain that reached `VerifiedClaim`, in chain order of its carrier.
/// `voided`: voided after verification (conviction, default, timeout).
/// `safe`: became safe at some block of this chain (§3.3); once set it never clears.
/// Final status is deliberately absent: it is a SafeDaa-timed fact and fork choice reads no clock.
pub struct VerifiedEntry { pub claim: ClaimId, pub carrier: BlockId, pub weight: Weight, pub voided: bool, pub safe: bool }

/// Everything fork choice may know about one candidate: a pure function of the candidate's chain
/// state. No store, no network, no clock. Claims that never reached `VerifiedClaim` do not appear.
/// (An implementation carries running totals instead of `seq`; they MUST equal §3.3.)
pub struct ChainView { pub tip: BlockId, pub seq: Vec<VerifiedEntry> }

/// A view proven to contain the anchor. Only `admit` constructs it, so a candidate that does not
/// descend from the finalized anchor cannot be passed to the comparator at all.
pub struct Admissible<'a> { view: &'a ChainView, anchor: &'a FinalizedAnchor }

/// The common safe context: the finalized anchor. ONE per selection; identical for every pair.
pub struct SafeContext { pub anchor: FinalizedAnchor }

/// A candidate's fork-choice weights (§3.3): absolute totals from genesis. No frontier: the claim
/// frontier that bounds pruning is 07's, not a fork key.
pub struct ForkWeights { pub safe: Weight, pub verified: Weight }

/// The per-candidate key: a function of (candidate, context) only.
pub struct ForkKey { pub safe: Weight, pub live: Weight, pub tie: BlockId }

pub struct ForkParams { pub beta: Ratio /* 0 < β ≤ 1 */, pub d_bury: Weight, pub d_dispute: Weight }
```

`Weight` is minted **only** by the claim state machine for `VerifiedClaim` and `FinalClaim` (the
typestate `UnverifiedClaim → VerifiedClaim → FinalClaim`). There is no constructor for `Weight`
from a header, an algo id, `bits`, a nonce, a block count or a clock.

### 2.4 Fork-choice projection of the authority table (03 §2.5)

| Fact (PoL's five facts) | Type | Fork-choice authority |
| --- | --- | --- |
| (a) the LLM was executed | off-chain | none |
| (b) a commitment/root is held | `UnverifiedClaim` | **none** — absent from `ChainView` |
| (c) a lottery was won | admission fact on `UnverifiedClaim` | **none** |
| (d) the panel verified the claim | `VerifiedClaim` | `live` (at `β`) and, as later weight, it **buries** earlier claims |
| (d′) the verified claim is buried by `d_bury` and not held by an unresolved dispute | `VerifiedClaim` or `FinalClaim` | `safe` (§3.3), permanently |
| (e) the claim settled (challenge window closed on `SafeDaa`) | `FinalClaim` | **nothing beyond (d)**: settlement moves money, seeds and rights, not weight |
| heartbeat / execution-lane / empty blocks | none | none |
| blue work, blue score, header `bits`, timestamps, `LocalDaa`, `SafeDaa` values | structure / clocks | none |

## 3. The design argument

### 3.1 The problem: maturity is time, and a branch owns its time

A claim becomes `FinalClaim` when its challenge window closes on its branch's own clock. On t12
that clock is `LocalDaa`. In next it is `SafeDaa` (03 CLAIM-R9), which is still a per-branch clock:
it copies `LocalDaa`'s spacing and moves with the branch's own licences (01 §2.3, S5). A branch that
advances its clock faster than the honest network closes its windows sooner. It would show more
settled weight at the same wall-clock moment. That is `private_daa_finality_acceleration`, and
INV-FORK-01 forbids it. The seed design answers it with a *common safe DAA*: compare every
candidate's settled work at one clock reading both have reached.

### 3.2 Why comparing at a common clock reading fails, in all three forms

Let `t_X` be candidate X's safe-clock reading and `s_X(p)` its settled weight at readings `<= p`.

**(i) Pairwise `min(t_A, t_B)` is not transitive.** Monotone profiles suffice for a cycle:

| candidate | `t` | `s(10)` | `s(20)` |
| --- | --- | --- | --- |
| A | 10 | 5 | — |
| B | 20 | 4 | 9 |
| C | 30 | 6 | 8 |

A vs B at 10: 5 > 4. B vs C at 20: 9 > 8. A vs C at 10: 5 < 6. **A > B > C > A**: the selection
depends on the order candidates are visited, and two nodes holding one DAG pick different tips
(`pairwise_context_cycle`).

**(ii) One minimum over the whole candidate set is transitive but can be dragged.** A one-block
fork at the anchor has `t = anchor`, forces the common reading to the anchor, zeroes every settled
key, and hands the decision to unsettled work:

| candidate | `t` | `s(100)` | verified total |
| --- | --- | --- | --- |
| H (honest) | 100 | 50 | 60 |
| A (attacker, forked at the anchor, clock raced to 140) | 140 | 10 | 70 |
| J (junk: one empty block on the anchor) | anchor | 0 | 0 |

Without J the reading is 100 and H wins (50 vs 10). With J it is the anchor, settled keys tie at 0,
and A's larger unsettled pile wins. One free block flipped the chain (`junk_candidate_context_drag`).

**(iii) A minimum over "contenders" only is dragged by a stalled leader.** Filtering out candidates
that another candidate beats on everything removes J, but not a branch that was briefly ahead and
then stopped: J2 settles weight 2 by reading 60 and produces nothing after; H has settled only 1 by
reading 60 and 500 by reading 1000. J2 is not beaten at its own reading, so it sets the common
reading to 60, and **J2 (total 2) beats H (total 500)** (`stalled_leader_context_drag`).

The common cause: a clock reading cannot tell a branch that is behind because it *raced nobody*
from a branch that is behind because it *stopped working*. Truncating at the laggard's reading
punishes the second kind of lead as hard as the first. So misaka-next does not truncate at a clock
reading at all.

### 3.3 The resolution: the context is the anchor, and maturity is burial by verified work

The common safe context is **the node's finalized anchor**. Every admissible candidate contains it,
it is the same for every pair, and it changes only when finality advances (chapter 07). It decides
who is compared. It is not a clock reading anything is truncated at. Maturity is measured on the
**work clock** `Ω`, the cumulative verified weight of a chain from genesis. **Whether a claim is
`Final` does not enter.** `Final` elapses on `SafeDaa`, and `SafeDaa` is a per-branch clock that
lags `LocalDaa` by the licence ring and can catch up in a burst. A key that counted `Final` would let
a branch whose `SafeDaa` ran ahead count settled weight first. The lag has no upper bound in a
licence drought, so no burial depth could calibrate that away.

For an admissible view with entries `e_1 … e_n`, the notions are these. The entries are every claim
on the chain that ever reached `VerifiedClaim`, in chain order of their carriers, with weights `w_i`
and `Ω_i = w_1 + … + w_i`, voided entries included.

```
buried(e_i, Y)  := Ω_Y − Ω_i >= d_bury                      at block Y of the chain (Ω_Y: weight verified by Y)
held(e_i, Y)    := some dispute on e_i opened at or before Y has neither a proven verdict at or
                   before Y, nor d_dispute of verified weight accepted after its opening, up to Y
safe(e_i)       := at some block Y of the chain: buried(e_i, Y) ∧ ¬voided(e_i, Y) ∧ ¬held(e_i, Y)
                   (once safe, always safe on that chain: W2)
safe(X)         := Σ w_i over entries that are safe
V(X)            := Σ w_i over entries that are safe or not voided
live(X)         := safe(X) + ⌊β · (V(X) − safe(X))⌋          (0 < β ≤ 1)
```

A **dispute** is a court or DA session opened on the claim. For fork choice it is released only by
a proven verdict object or by burial under `d_dispute` of verified weight. A default does not
release it, because defaults are swept on a clock (05 PANEL-R21). A default that convicts voids the
entry, which can only lower the branch's keys. Every term above is a function of the chain's objects
and verified weight. None reads `LocalDaa`, `SafeDaa`, `ChainFinalizedDaa` or time.

Keys are absolute totals, like proof-of-work chain work. Whatever two candidates share contributes
equally to both and cancels in the comparison, so no "measured from" point is needed and moving the
anchor changes no key. An implementation maintains `Ω`, the `safe` flags, `safe` and `V`
incrementally in the chain state. At each block it sets `safe` on every entry that the block's new
weight buries, that is not voided and that no unreleased dispute holds. The incremental values MUST
equal the definition.

Burial counts work that was verified even if it is voided later. A late conviction therefore removes
an unsafe entry's own weight from `live` but never un-buries matured work beneath it. That is
ADR-0039 §3e's rule, "a conviction can never rewrite safe weight"
(`docs/adr/0039-palw-only-block-production.md:227-232`). Safe flags only turn on and `Ω` only grows,
so `safe` only grows along a chain (W2), and so does every count built on it (07 FINAL-R6).

The **claim frontier** that bounds pruning is a different object. It covers all claims, including
unverified claims, disputed ones and `Final` claims whose conviction window is open. It is 07's
(`own_frontier`, 07 §2.3), not a fork key. *[Synthesis edit, review: the draft defined a "safe
frontier" here over verified entries only, which disagreed with 03 A5 and 07 FINAL-R10.]*

`SafeDaa` itself is chapter 01's (DAA-R11: minted from the licence ring, not from settled claims;
minting it from `FinalClaim`, which reads `SafeDaa`, would be circular). Fork choice never reads it
(§2.2). *[Synthesis edit: an earlier draft defined `SafeDaa` here as the `LocalDaa` of the
safe-frontier block.]*

**Calibration (FORK-R16).** Two burial depths are calibrated through W1(a):

* `d_bury ≥ ρ · W_open + b`, where `W_open` is the span, in `LocalDaa` ticks, an honest party needs
  to observe a verified claim and open a dispute: the challenge window `W_challenge(class)`, maximised
  over classes, plus the delivery bound `Δ` in slots.
* `d_dispute ≥ ρ · W_court + b`.

W1(a) turns weight into ticks, and INV-DAA-02 turns ticks into a lower bound on wall-clock slots. So
on a branch running at any speed, an entry counts in `safe` only after at least `W_open` slots of
real time since its acceptance, during which an honest challenger could open a dispute and hold it.
A dispute holds for at least `W_court` slots of real time unless a proof closes it first. Neither
bound reads a clock, and neither depends on how far `SafeDaa` lags. t12 already uses a count of
verified events for its economic deadlines: they wait for their DAA window *and* thirty further
settled anchors (`consensus/core/src/config/params.rs:14700-14711`).

### 3.4 Properties, each with its argument

* **Total and transitive.** `ForkKey` is a function of the candidate only, and it includes the tip
  id; the order is the pullback of a lexicographic product of total orders along an injective map,
  hence a strict total order on distinct tips (INV-FORK-02).
* **Independent of the rest of the set.** The order between two candidates never depends on a third
  (INV-FORK-04). Adding, removing or extending other candidates cannot flip it. §3.2's drags have
  nothing to drag.
* **Junk-proof.** J is the anchor block: its entries are exactly the ones every candidate shares, so
  it ties H on the common part and loses on everything H verified since.
* **Stalled-leader-proof.** J2's two claims are the last verified work on its chain; nothing buries
  them, so they add nothing to `safe(J2)` (for `d_bury > 0`), while H's buried work does.
* **Race-proof (INV-FORK-01), exactly.** Let `A'` be `A` with the same verified content on a faster
  `LocalDaa`: the same verified entries, dispute openings and proven verdicts, in the same chain
  order. Then `Ω`, burial and holds are identical at corresponding blocks, since none reads a clock.
  Voids are the only clock-driven input: defaults and timeouts are swept on `SafeDaa` or
  `ChainFinalizedDaa`, and those can only run ahead on `A'`. So `A'` voids every entry `A` voids, no
  later. An entry that becomes safe on `A'` at some block was buried, not held and not voided there,
  and therefore also not voided on `A` there. So `safe(A') ≤ safe(A)` and `V(A') ≤ V(A)`, hence
  `live(A') ≤ live(A)`. Racing the clock never raises a key.
* **Maturing is monotone.** Burying an entry or releasing a hold moves weight into `safe`, and
  `live = (1−β)·safe + β·V` rises with it (β ≤ 1). A node never reorganises away from the chain
  that just improved (INV-FORK-08; the t12 insight, kept). Only a void of an unsafe entry lowers a
  key.
* **Anchor moves change no key.** Keys are absolute totals and the tie key is the tip id; the
  anchor enters only through admissibility. Advancing it removes candidates that do not contain the
  new anchor and changes nothing about the order of the rest (INV-FORK-09).
* **No incumbent.** The node's previous tip is not an input (INV-FORK-03). A reorg is "a different
  argmax of the same pure function over a new set", never a consequence of which chain was held.
* **Not a bound on a private branch's own verified weight.** Race-proofness compares branches with
  the *same* verified content. A private branch has different content: every licence on it comes
  from the adversary's own seats (10 §2.1 C11). No honest party can dispute its fabricated claims, so
  they are buried and become safe. What keeps that branch lighter than the honest chain is the rate
  condition of 10 §2.2 H2, `P_cap(s) · ρ < R_honest` with a margin, and nothing in this chapter.

### 3.5 The seed order, mapped

| Seed step | Here |
| --- | --- |
| `compare_final_anchor` | `admit` (FORK-R1): a candidate without the anchor is not compared |
| `compute_common_safe_daa` | `SafeContext::from_anchor` (FORK-R3): the anchor, i.e. `FinalizedDaa`; no per-set or per-pair clock reading (§3.2); maturity is burial on the work clock (§3.3) |
| `compare_safe_frontier` + `compare_safe_weight` | one key, `safe` (FORK-R6): measured in weight, the depth of matured (buried, undisputed) work *is* the safe weight; a second key in another unit (blocks, a clock reading) is t12's T12-D2 |
| `compare_live_weight` | `live` (FORK-R7) |
| `deterministic_tiebreak` | `tie` = tip id (FORK-R5) |

`UnverifiedClaim` contributes to none of them (INV-POL-01): a lottery won on a fake root, a burst of
unverified attempts, a failed lottery, a heartbeat — all are invisible to fork choice.

## 4. Normative rules

* **FORK-R1 — Admissibility.** A candidate whose chain does not contain the finalized anchor MUST
  NOT be compared; `admit` returns `None` for it and it is not in the candidate set.
  *Because:* INV-FINAL-01, `long_range_rewrite`.
* **FORK-R2 — Canonical candidate set.** The candidate set `C` MUST be exactly the fully validated
  chain tips known to the node that contain the anchor; if none qualifies, the anchor itself is the
  only candidate. `C` MUST NOT depend on arrival order, peer identity or the previous selection.
  *Because:* `path_dependent_sink_split`.
* **FORK-R3 — The context is the anchor.** Every comparison MUST use `SafeContext { anchor }` for the
  node's finalized anchor. No comparison may derive a clock reading from the candidates it compares
  or from the candidate set. *Because:* `pairwise_context_cycle`, `junk_candidate_context_drag`,
  `stalled_leader_context_drag`.
* **FORK-R4 — Maturity is burial.** A verified claim MAY count toward `safe` only once it is safe
  (§3.3): at some block of the chain, at least `d_bury` of verified weight follows it, it is not
  voided, and no dispute on it is unreleased. A dispute is released only by a proven verdict or by
  `d_dispute` of verified weight after its opening. Once safe, it stays safe. Whether the claim is
  `Final` MUST NOT enter. Burial weight MUST include entries voided after verification. *Because:*
  INV-FORK-01. `Final` elapses on `SafeDaa`, which a racing branch moves (01 S5). A late conviction
  must not rewrite matured history.
* **FORK-R5 — Key order.** Candidates MUST be ordered lexicographically by `safe`, then `live`
  (larger wins), then `tie` = the tip's `BlockId` (smaller wins). No key may be inserted.
  *Because:* INV-FORK-02.
* **FORK-R6 — Safe weight.** `safe` MUST be the total weight of safe entries on the chain (§3.3),
  and nothing else. *Because:* anti-fabrication: a heavy unmatured pile cannot outrank matured work.
* **FORK-R7 — Verified-only live weight.** `live` MUST be `safe + ⌊β·(V − safe)⌋`, with `V` the
  weight of entries that are safe or not voided. `UnverifiedClaim`s MUST contribute zero, and so MUST
  claims voided before they became safe. *Because:* INV-POL-01, `private_fake_root_burst`,
  `unverified_live_weight`.
* **FORK-R8 — Clock typing.** No key and no context may take `LocalDaa`, `SafeDaa`, blue score, blue
  work, a header timestamp or wall-clock time as input. The signatures in §5 make this a type error.
  *Because:* `heartbeat_clock_acceleration`, `heartbeat_padding_buys_frontier_key`.
* **FORK-R9 — No header-only weight.** No quantity derivable from a header alone (algo id, `bits`,
  nonce, commitment shape, block level) may contribute to any key. There is no block-level work at all
  (01 DAA-R9, 03 CLAIM-R3). Weight enters only through the verified state transition of the chain
  being weighed; a merged block's work counts only once the merging chain's state has verified it.
  *Because:* `failed_lottery_blue_weight`.
* **FORK-R10 — One comparator, every site.** Every chain-selection decision — the selected tip, a
  block template's parent, IBD / staging adoption, headers-proof acceptance, restart recovery,
  bootstrap recovery and (if the block structure is a DAG) the selected parent of a block — MUST be
  `select_tip` / `compare_chains` over admissible views. A download-ordering hint MAY exist but MUST
  NOT be read by any consensus or finality decision. *Because:* INV-FORK-06.
* **FORK-R11 — No hysteresis.** The previously selected tip MUST NOT be an input. "Keep the
  incumbent unless the challenger strictly wins" is permitted only as the literal consequence of
  FORK-R5 (two distinct tips never compare equal). *Because:* INV-FORK-03.
* **FORK-R12 — Unweighable is absent.** A candidate whose `ChainView` cannot be derived MUST be
  excluded from `C`; the node MUST NOT order it by anything else. It is neither "zero" nor
  "maximum". *Because:* `unweighable_fail_open`.
* **FORK-R13 — No node-local inputs.** No runtime flag, memo, cache state, "abstain" condition, peer
  score or operator setting outside the consensus parameters may change the result. An overlay
  (a validator vote, an operator checkpoint) MAY influence fork choice only by moving the finalized
  anchor through chapter 07's rules. *Because:* `dns_gate_node_local_abstain`.
* **FORK-R14 — Staging uses the same anchor.** IBD MUST weigh the local and the staged candidates as
  members of one candidate set from one anchor. If the two sides' finalized anchors are not on one
  chain, that is a finality conflict (FINAL-R9) and MUST NOT be resolved by fork choice.
  *Because:* `ibd_asymmetric_weighing`.
* **FORK-R15 — Exact and incremental.** Implementations MUST maintain `Ω`, `safe` and `V` in the
  chain state so that `fork_key` is O(1) per candidate, and the maintained values MUST equal §3.3's
  definition (no sampling, no cache that is not a pure function of the chain). *Because:*
  determinism across implementations.
* **FORK-R16 — Calibration.** `validate_params` MUST compute `ρ` and `b` from W1(a) and MUST refuse
  a ruleset in which any of these holds:
  * `W_max` or `CCU_max` is missing (so `ρ` would be unbounded);
  * `d_bury < ρ · W_open + b` or `d_dispute < ρ · W_court + b` (§3.3);
  * `k_final < D_SAFE` (07 FINAL-R2);
  * `β` is outside `(0, 1]`.

  *Because:* INV-FORK-01; a calibration against an unbounded `ρ` is no calibration.

## 5. Pure functions

```rust
/// Views that contain the anchor, and only those (FORK-R1).
pub fn admit<'a>(view: &'a ChainView, anchor: &'a FinalizedAnchor) -> Option<Admissible<'a>>;

/// FORK-R3. Reads no candidate.
impl SafeContext { pub fn from_anchor(anchor: &FinalizedAnchor) -> SafeContext; }

/// FORK-R4/R6/R7. Absolute totals; reads no anchor.
pub fn fork_weights(view: &Admissible<'_>, params: &ForkParams) -> ForkWeights;

/// FORK-R5..R7. A function of one candidate and the context.
pub fn fork_key(view: &Admissible<'_>, ctx: &SafeContext, params: &ForkParams) -> ForkKey;

/// `Greater` means `a` is preferred. Pure, total, antisymmetric, transitive.
pub fn compare_chains(a: &Admissible<'_>, b: &Admissible<'_>, ctx: &SafeContext, params: &ForkParams) -> Ordering;

/// The maximum under `compare_chains`. `None` only for an empty slice, which FORK-R2 rules out.
pub fn select_tip(ctx: &SafeContext, candidates: &[Admissible<'_>], params: &ForkParams) -> Option<BlockId>;
```

Reference semantics:

```text
fork_weights(X):                              // e.safe is the chain-state flag of §3.3 (set once, never cleared)
    safe = 0; V = 0
    for e in X.seq:                           // chain order from genesis
        if e.safe: safe += e.weight
        if e.safe || !e.voided: V += e.weight
    return ForkWeights { safe, verified: V }

maintain_safe_flags(X, block Y):              // run by the per-block transition (11 §4 step 9)
    for e in X.seq with !e.safe:
        if Ω_Y − Ω_e >= d_bury && !e.voided && !held(e, Y): e.safe = true

fork_key(X, ctx):
    (s, V) = (fork_weights(X).safe, fork_weights(X).verified)
    live = s + floor(β * (V - s))             // saturating; β a rational
    tie  = X.tip                              // BlockId, compared bytewise
    return (s, live, tie)

compare_chains(a, b, ctx) = (a.safe, a.live).cmp(&(b.safe, b.live)).then_with(|| b.tie.cmp(&a.tie))

select_tip(ctx, C) = argmax over C of compare_chains(·, ·, ctx)
```

Expected properties (each a named test in `09-invariants.md`):

| Property | Statement |
| --- | --- |
| symmetry | `compare_chains(a,b) == compare_chains(b,a).reverse()` |
| totality | `compare_chains(a,b) == Equal` iff `a.tip == b.tip` |
| transitivity | `a>b ∧ b>c ⇒ a>c` for all admissible a, b, c |
| determinism | same inputs, same output; no hidden state (no `&mut`, no store handle) |
| permutation invariance | `select_tip` is unchanged by any permutation of `candidates` |
| independence of irrelevant candidates | `compare_chains(a,b)` does not depend on `C \ {a,b}` |
| private-DAA invariance | INV-FORK-01 (§6) |
| reorg invariance | the selected tip is a function of (anchor, C) only; the previous tip is not an input |
| anchor-move invariance | advancing the anchor changes no key; it only removes candidates that do not contain it |
| monotone maturing | burying an entry or releasing a hold never lowers `fork_key` |
| clock blindness | `fork_key` has no argument of a clock type; changing only clock-driven timing can only add voids |

## 6. Invariants upheld

* **INV-FORK-01** (seed) A branch MUST NOT gain comparative maturity solely because its private DAA
  is ahead of the competing branch. *Formal:* let `A'` equal `A` except that its `LocalDaa` advances
  faster, with the same verified content (the same verified entries, dispute openings and proven
  verdicts in the same chain order). Then `safe(A') ≤ safe(A)` and `live(A') ≤ live(A)` at
  corresponding blocks (§3.4). Depends on W1 (INV-CLAIM-01) and FORK-R16 only for the real-time
  meaning of burial, not for the inequality. Test
  `inv_fork_01_private_daa_does_not_buy_comparative_maturity`. *[Synthesis edit, review: the draft
  counted `Final` claims in `safe` and allowed a gain up to the weight of disputed buried claims.
  With a `SafeDaa`-timed challenge window, burial did not imply settlement.]*
* **INV-FORK-02** `compare_chains` is a strict total order on distinct admissible tips.
  Test `inv_fork_02_comparator_is_a_strict_total_order`.
* **INV-FORK-03** Selection is a function of (finalized anchor, candidate set) only: no incumbent, no
  arrival order, no node-local state. Test `inv_fork_03_selection_is_path_independent`.
* **INV-FORK-04** The relative order of two candidates does not depend on any other candidate.
  Test `inv_fork_04_no_candidate_can_move_the_order_of_two_others`.
* **INV-FORK-05** No block without verified work increases `Ω` or `V`: not a heartbeat, an
  execution-lane block, a failed or skipped attempt, or an unverified claim. Such a block increases
  `safe` only if it carries a proven verdict that releases a hold. Its passage of time can only void
  pending claims, which lowers `live`, so a branch cannot improve its weights by producing it.
  Test `inv_fork_05_blocks_without_verified_work_add_no_weight`.
* **INV-FORK-06** Every chain-selection site (FORK-R10 list) returns the same tip for the same
  inputs. Test `inv_fork_06_every_selection_site_agrees`.
* **INV-FORK-07** A candidate that does not contain the finalized anchor is never selected.
  Test `inv_fork_07_no_candidate_below_the_finalized_anchor_is_selected`.
* **INV-FORK-08** Burying a claim or releasing a hold never lowers a chain's key.
  Test `inv_fork_08_maturing_never_lowers_the_key`.
* **INV-FORK-09** Advancing the finalized anchor leaves the pairwise order of every candidate that
  contains the new anchor unchanged. Test `inv_fork_09_anchor_advance_preserves_the_order`.
* Upheld from other chapters: **INV-POL-01** (FORK-R7), **INV-FINAL-01** (FORK-R1).

## 7. t12 reference

### 7.1 The real t12 order (read from code, a V2 network such as testnet-12)

The comparator `compare_palw_candidates_v1` (`consensus/core/src/palw_fork_choice.rs:72-78`) orders
`safe_frontier_blue_score`, then `safe_weight`, then `live_total`, then the candidate hash. **It is
not the selector.** The virtual sink is chosen by `sink_search_algorithm`
(`consensus/src/pipeline/virtual_processor/processor.rs:13594`), in this order:

0. **DNS stake preference** (`processor.rs:13624`) — inert on t12: `PALW_T12_DNS_PARAMS` is
   `PRODUCTION_DNS_PARAMS` at two-minute cadence (`consensus/core/src/config/params.rs:9519-9525`),
   production's multiplier is 0 (`params.rs:9373`), and `dns_stake_preferred_tip` returns `None` for
   0 (`processor.rs:13526-13528`).
1. **Heap by GHOSTDAG blue work, then hash** (`processor.rs:13641-13644`;
   `consensus/core/src/sortable_block.rs:51-54`). The heaviest tip is popped first.
2. **Kaspa finality**: a candidate not in the future of the virtual finality point is skipped
   (`processor.rs:13704`), finality depth 600 blue score (chapter 07).
3. **UTXO / PALW state validity** of the candidate (`processor.rs:13721-13722`).
4. **`dns_reorg_outcome`** (`processor.rs:13724`, body `processor.rs:13187-13250`):
   * 4a. **DNS BFT gate first** (`processor.rs:13200`; `dns_bft.rs:530-567`): a candidate — reorg or
     extension — that does not contain the DNS-final anchor is refused, unless the anchor is stale by
     `dns_veto_ttl_daa_score` (120, `consensus/core/src/dns_finality.rs:1464`) measured on the
     *incumbent's* DAA (`dns_bft.rs:532, 554`); the gate abstains while this node's own evaluation
     walk failed (`dns_bft.rs:457, 534-536`). Armed on t12 from DAA 0 (`params.rs:15562-15563`,
     walked to 0 by `palw_t12_arm_every_rule_from_genesis`, `params.rs:15751`, whose `for_each_fence`
     visits the gate's activation at `params.rs:6996-6997`).
   * 4b. **Only if the candidate is not a chain descendant of the previous sink**
     (`processor.rs:13219`): `decide_deep_reorg_v2(incumbent = prev_sink, challenger)`
     (`processor.rs:13234-13249`; `palw_fork_authority_v2.rs:94-99`), i.e. the comparator, as a
     **veto**; an unweighable side refuses. On `Allow`, ADR-0065 D2 frontier provenance
     (`processor.rs:3733`), dormant on t12 (`consensus/core/tests/hb_fork_weight.rs:62`;
     `params.rs:15736`).
   * 4c. Otherwise (an extension) the legacy DNS gate, which accepts a candidate containing the
     anchor (`processor.rs:13253-13290`).
5. The first accepted candidate is the sink (`processor.rs:13725-13753`).

So yes: **a DNS BFT gate stands ahead of the PALW comparator**, and **blue work stands ahead of
both**. The in-tree adjudication test says the same in its own words: "the comparator is a veto,
not a promoter" (`consensus/core/tests/hb_adjudication.rs:64-121`).

### 7.2 Every chain-selection site in t12

| Site | What decides | Citation |
| --- | --- | --- |
| virtual sink | blue-work heap, then DNS-BFT veto, then comparator veto on reorgs only | `processor.rs:13594-13790` |
| block's selected parent | max blue work among parents | `consensus/src/processes/ghostdag/protocol.rs:216-220` |
| deep reorg | comparator (`decide_deep_reorg_v2`) after the DNS-BFT gate | `processor.rs:13200-13249` |
| IBD commit | comparator, incumbent weighed at its sink, challenger at the peer's pruning point | `protocol/flows/src/ibd/flow.rs:1963-2011`; `consensus/src/consensus/mod.rs:2479-2482`; `processor.rs:3931-3951` |
| headers-proof acceptance | blue work, using the header-selected tip's blue work | `consensus/src/processes/pruning_proof/validate.rs:488-551` (492) |
| pruning point (header-committed) | blue-score depth | `consensus/src/processes/pruning.rs:106-156`; `consensus/src/pipeline/virtual_processor/utxo_validation.rs:1070-1072` |
| pruning point (local store) | ceiling at the safe frontier | `consensus/src/pipeline/pruning_processor/processor.rs:207-229`; `processor.rs:3958-3978` |
| restart recovery | `select_palw_tip_v2` exists, **no production caller** (grep over the tree finds only its definition and tests) | `consensus/core/src/palw_fork_authority_v2.rs:43-45` |
| bootstrap (provisional-chain) adoption | blue work (`SortableBlock`) | `protocol/flows/src/flowcontext/bootstrap_recovery.rs:274-300` |
| header-selected tip | blue work, declared a download hint | `consensus/src/pipeline/header_processor/processor.rs:485-500` |

ADR-0042 Decision 9 requires the comparator at "virtual canonical tip, IBD-complete tip, pruning
point, finality/deep-reorg gate, restart recovery, sync-peer chain comparison"
(`docs/adr/0042-palw-mainnet-candidate-ruleset.md:438-451`). t12 meets it at the deep-reorg gate and
the IBD commit, partially at pruning (a ceiling, not a selection), and not at the tip, restart or
sync-peer sites.

### 7.3 Defects and how next differs

* **T12-D1 — path-dependent selection.** Because the comparator only vetoes *reorgs relative to the
  previous sink* (`processor.rs:13219`) and the heap offers by blue work, two nodes holding the same
  DAG can settle on different sinks. Example: tip P is heavier in blue work with a lower frontier,
  tip M lighter with a higher frontier. A node whose sink is on M's chain pops P, sees a reorg, the
  comparator refuses it, and keeps M. A node whose sink is on P's chain pops P, sees an extension of
  its own sink (no comparator), and keeps P. Neither will move until blue work or frontier changes.
  *next:* FORK-R2, FORK-R11, INV-FORK-03.
* **T12-D2 — the frontier is a block count.** `safe_frontier_blue_score` is set to the settled
  claim's `accepted_blue_score` (`consensus/core/src/palw_state_v2.rs:20935`) and compared first
  (`palw_fork_choice.rs:73`). Blue score is advanced by any blue block, including heartbeats and
  (as merged blues) attempt headers that failed admission (T12-D5). The in-tree lane test prices the
  flip at "one beat per honest matured block" on constructed orders
  (`hb_fork_weight.rs:119-174`); since the clock floor (`params.rs:15909`) a heartbeat chain is paced
  at one slot per 120 s (`consensus/src/pipeline/header_processor/pre_pow_validation.rs:84, 95`), so
  the heartbeat part of that price is now wall-clock time **[the sustained padding rate on the live
  preset is unverified; needs a probe]**. *next:* matured depth is buried verified weight,
  compared as weight, never as a block count or clock reading (FORK-R4, FORK-R6, FORK-R8).
* **T12-D3 — no common context.** Each side's order is its own tip's state
  (`processor.rs:3691-3702`); claim maturity is `licensed_daa + window_challenge_at(licensed_daa)` on
  the branch's own DAA (`palw_state_v2.rs:3233-3243`), swept at `deadline < ctx.daa_score`
  (`palw_state_v2.rs:23666`, finalize at `23771`). On t12 the DAA itself is paced by the ADR-0142
  cursor (below), so this is not directly exploitable there; it is a design dependency next removes
  by measuring maturity as burial (FORK-R4). t12's own second clock is the precedent: economic
  deadlines there also wait for thirty further settled anchors (`params.rs:14700-14711`, armed at
  `params.rs:15901`; `palw_state_v2.rs:2224-2229`), but fork choice does not read it.
* **T12-D4 — unverified claims carry live weight.** A claim's `immature_contribution = β·pwu/1000`
  is priced at acceptance (`palw_state_v2.rs:28436-28443`) and added to `bounded_immature` by
  `reserve_for_claim` (`palw_state_v2.rs:18618, 18650-18654`), which feeds `live_total`
  (`palw_fork_choice.rs:62-64`). A `Provisional` claim no panel has seen is therefore key 3.
  *next:* FORK-R7.
* **T12-D5 — header-only blue work.** Any attempt-algo header earns `2^20` blue work
  (`protocol.rs:667-671`) — "a constant and NOT the envelope's claimed pwu"
  (`protocol.rs:630`). The header stage checks the envelope, signature, DA pins and nonce bucket, but
  not the bond or the class lottery (`consensus/src/pipeline/header_processor/pre_ghostdag_validation.rs:310-367`;
  "whether that key is the named bond's is admission item 2's stateful question", `:341`). On t12
  the attempt lane pays no `bits` target at all (`consensus/pow/src/lib.rs:594-596`). The lottery
  runs only in the virtual processor (`consensus/core/src/palw_admission_v2.rs:804-814`, via
  `processor.rs:11505`); a merged attempt that fails is skipped and "nothing about its anticone may
  disqualify the accepting block" (`processor.rs:11598, 11647-11649`), while its `2^20` is already in
  the merging block's blue work (`protocol.rs:340-355`). *next:* FORK-R9, INV-FORK-05.
* **T12-D6 — node-local veto ahead of the comparator.** The DNS-BFT gate reads a runtime flag set
  by this node's own evaluation (`dns_bft.rs:457, 534-536`), the node's own sink DAA (`:532`) and
  the node's own DNS state store; two honest nodes can answer differently for one candidate.
  *next:* FORK-R13; overlays act only through the anchor (chapter 07).
* **T12-D7 — IBD compares unlike things.** The incumbent is weighed at its sink and the staged
  chain at the pruning point whose carriage was imported (`mod.rs:2479-2482`;
  `processor.rs:3931-3951`), after a headers proof accepted by blue work (`validate.rs:488-551`).
  *next:* FORK-R14.
* **T12-D8 — sites the comparator never reached.** Restart recovery has a function
  (`select_palw_tip_v2`, `palw_fork_authority_v2.rs:43-45`) and no production caller; the
  provisional-chain adoption after a first IBD orders by blue work
  (`bootstrap_recovery.rs:274-300`); headers-proof acceptance orders by blue work read from the
  header-selected tip (`validate.rs:488-493`), which the header processor calls a download hint
  (`header_processor/processor.rs:485-500`). *next:* FORK-R10.
* **T12-D9 — the chain's shape is chosen by blue work.** Every block's selected parent is its
  heaviest parent by blue work (`protocol.rs:216-220`), so the chain on which PALW state is folded,
  below whichever tip wins, is shaped by the quantity T12-D5 shows is header-only. *next:* FORK-R10,
  §9 Q5.
* **Kept from t12:** one comparator function with a total tie-break; `live = safe + bounded` so
  maturing is monotone (`palw_fork_choice.rs:24-27, 62-64`); fail-closed on an unweighable side
  (`processor.rs:13234-13248`); strict win required to replace (`palw_fork_authority_v2.rs:59-64`);
  the frontier never retreats (`palw_state_v2.rs:20947` debug assertion); matured-work-first
  ordering (ADR-0039 §3b-3c, `docs/adr/0039-palw-only-block-production.md:177-207`).

### 7.4 The t12 DAA clock, for the verdicts

On t12 no producible lane advances DAA by itself: past `palw_single_lottery` only lanes priced by
`bits` advance it (`consensus/src/processes/difficulty.rs:705-722`), and the in-tree test shows none
is (`hb_adjudication.rs:44-62`). The score advances by at most one heartbeat stand-in per mergeset
(`difficulty.rs:459`), granted only at the cursor slot (`difficulty.rs:502`), derived from header
timestamps at least 120 s apart (`pre_pow_validation.rs:84-97`), and a timestamp may not exceed local
time plus the deviation tolerance (`pre_ghostdag_validation.rs:434-438`). So on t12 a private branch
cannot run its DAA meaningfully faster than wall time. That last check reads the wall clock, which a
pure consensus function may not; in next it is a delivery rule (a block from the future is not yet
delivered), and fork choice does not rely on it (FORK-R8).

### 7.5 Pending deltas

None of the pending branches in `PROVENANCE.md` touch `palw_fork_choice.rs`,
`palw_fork_authority_v2.rs`, `dns_bft.rs`, the sink search or GHOSTDAG (checked with
`git diff --stat rcore/int-3...<branch>`). `feat/t12-class-verify-deadline` changes the Final floor
to `max(L + window_challenge_at(L), H(c))` (*pending*, `palw_claim_final_floor_v1`), still on the
branch's own DAA.

## 8. Attacks this chapter defends against

Detailed in `10-attack-model.md`. t12 verdicts are from the code read above.

| Attack | t12 | next defence |
| --- | --- | --- |
| `private_daa_finality_acceleration` | **partial** — DAA is wall-clock paced (§7.4), but maturity is per-branch with no common context and key 1 is blue score | FORK-R3, FORK-R4, FORK-R8, FORK-R16, INV-FORK-01 |
| `failed_lottery_blue_weight` | **real** for GHOSTDAG blue work (T12-D5); **closed** for PALW safe/live weight (a claim is created only by admission); blue work decides the heap, the selected parent and headers-proof acceptance | FORK-R9, FORK-R10, INV-FORK-05 |
| `heartbeat_padding_buys_frontier_key` (this chapter's earlier name: `frontier_blue_score_padding`) | **partial** (T12-D2; the comparator is reached only after the blue-work heap offers the branch, `hb_adjudication.rs:64-121`) | FORK-R6, FORK-R8 |
| `heartbeat_clock_acceleration` | **partial** — DAA stand-in is cursor-bounded; heartbeats still add blue score to the frontier key and to every blue-score depth | FORK-R8, INV-FORK-05 |
| `private_fake_root_burst` | **partial** at fork choice — a winning fake claim is a valid chain block with `2^20` blue work and key-3 live weight before any panel (T12-D4, T12-D5); no safe weight until licensed and settled | FORK-R7, FORK-R9 |
| `unverified_live_weight` | **real** (T12-D4) | FORK-R7 |
| `path_dependent_sink_split` | **real** (T12-D1) | FORK-R2, FORK-R11 |
| `dns_gate_node_local_abstain` | **partial** (T12-D6; needs a confirmed DNS-final anchor on the network) | FORK-R13 |
| `ibd_asymmetric_weighing` | **partial** (T12-D7) | FORK-R14 |
| `pairwise_context_cycle` | not applicable (t12 has no context) | FORK-R3 |
| `junk_candidate_context_drag` | not applicable (t12 has no context) | FORK-R3 |
| `stalled_leader_context_drag` | not applicable (t12 has no context) | FORK-R3, FORK-R4 |
| `unweighable_fail_open` | **closed** (`processor.rs:13234-13248`; `flow.rs:1985-2001`) | FORK-R12 |
| `private_self_licensing_branch` | **real** (05 §7: on its own branch the forker re-rolls every panel seed and licenses its own claims) | not by this chapter: 10 §2.2 H2 (`P_cap(s) · ρ < R_honest`), 04 POL-R9, OQ-26 |
| `dispute_hold_griefing` | not applicable (t12's key counts `Final`, not holds) | FORK-R4's hold is priced by accuser exposure (02 BOND-R6) and bounded per claim (05 COURT-R5); OQ-24 |

## 9. Open questions for the project owner

1. **β, the discount on verified-but-unmatured work.** Options: (a) β = 1; (b) a fixed fraction such
   as 1/2; (c) β → 0 (a pure tie-break among equal safe weights). *Recommendation:* (b). β > 0 keeps
   the network on the chain whose claims panels have licensed while burial accrues; β < 1 keeps a
   colluding-panel pile from matching buried work on `live`.
2. **`d_bury`: weight or count, and how large.** Options: (a) weight, calibrated by FORK-R16 from
   the admission budget and the per-claim ceiling `w_max`; (b) a count of verified entries, which is
   independent of `W` and needs no `W_max`; (c) both. *Recommendation:* (a), with (b) as an
   additional floor only if the claims chapter allows single claims so heavy that one of them could
   bury a whole window. *Synthesis note (review):* under (a) the burial depth is sized for `w_max`,
   so while `W` sits well below `W_max` burial takes up to `w_max / w` times longer in real time.
   (b) avoids that cost at the price of a second unit in 07's depth rule.
3. **Dispute holds on the work clock.** *Adopted in synthesis as (a), in its fork-choice form.* A
   dispute holds an entry out of `safe` until a proven verdict or `d_dispute` of verified weight
   (§3.3). Court and DA deadlines stay on `SafeDaa` or `ChainFinalizedDaa` for charging (05
   PANEL-R21), but no default releases a fork-choice hold. INV-FORK-01 is then exact. Options kept
   for the record: (b) keep clock deadlines and accept a bounded gain; (c) exclude any claim that
   ever had a dispute from `safe`, which hands every accuser a permanent veto. The cost of (a) is
   `dispute_hold_griefing` (10 §6): an accuser can hold an honest entry out of `safe` for
   `d_dispute` of weight. That is priced by the accuser exposure a refuted accusation forfeits (02
   BOND-R6) and bounded by the per-claim session cap (05 COURT-R5). The owner should confirm that
   price (OQ-24).
4. **Should any common clock reading survive as a secondary key?** The seed order had
   `compute_common_safe_daa`. Options: (a) none (this chapter); (b) set-wide minimum; (c) contender
   minimum. *Recommendation:* (a); §3.2 shows (b) and (c) are dragged by one cheap candidate.
5. **DAG selected parent.** If next keeps a DAG, the selected parent must be chosen by the
   comparator (FORK-R10). Options: (a) a linear chain with merged side-blocks carrying no weight;
   (b) DAG with selected parent = `select_tip` over the parents, from the anchor visible in the
   block's past; (c) keep GHOSTDAG blue work for parent selection only. *Recommendation:* (b);
   (c) re-creates T12-D5 and T12-D9.
6. **Should an overlay (DNS validators' BFT vote) exist at all in next?** Options: (a) none — PoL
   depth only, ADR-0127's direction (PALW settles on its own,
   `docs/adr/0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md`); (b) an input that
   can only move the finalized anchor forward (chapter 07, FINAL-R13); (c) t12's veto with a TTL.
   *Recommendation:* (a) for the core, (b) if wanted later, never (c) (FORK-R13).
7. **Tie-break.** Options: (a) the raw tip id; (b) `H(domain || anchor || tip)`. *Recommendation:*
   (a): a producer can grind its tip id under either, which matters only when two candidates tie on
   both weights, and (b) would make the order depend on the anchor and break INV-FORK-09's
   exactness.
