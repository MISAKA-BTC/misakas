# ADR-0039: PALW-only block production — a Base class instead of a hash floor, and a two-weight fork choice

Status: **Proposed.** Activates nothing. Supersedes **ADR-0038 W4 and W6** (restated below as
W4′ and W6′) and amends its Decision B and Decision E. Everything else in ADR-0038 — the layer
inversion, sampled verification, the court, per-class DAA, bonded producers — stands.

Date: 2026-08-17
Relates to: ADR-0038 (the layer inversion this completes), ADR-0036 (lineage and the mainnet
identity requirement), ADR-0027/0028 (refutation and sampling), ADR-0030–0033 (the arithmetic
court, and the reason the Base class comes first), ADR-0034 (classes and bands),
`docs/palw-mainnet-readiness-audit-2026-08-16-ja.md` (the re-audit whose blockers 1–4 this ADR
answers), branch `palw-only-v4` commits `7cc6b08`, `b385803`, `92f563d`, `04c2650`.

## Context — what "PALW is the consensus work" has to mean

ADR-0038 made PALW the primary consensus work but kept a hash **anti-stall floor** that produces
blocks when every class is unavailable (W6), and kept `spam_hash_work` as a permanent additive
term in `weight(B)` (W4). Both are hash paths to the two things that define consensus
participation: **the right to produce a block**, and **weight in fork choice**. While either
exists, "the scarce resource that orders blocks is inference" is conditional.

Three uses of hashing are *not* in question and are retained: block and transaction identity,
Merkle commitments, artifact and model pinning, and compressing signature inputs. Removing those
would discard half the machinery of a blockchain for no security gain. What this ADR removes is
narrower and exact:

1. `H(header) < target` as a **block-production right**;
2. the hash floor as an **emergency block-production path**;
3. cheap hash as a **fork-choice weight** term.

## The impossibility, stated rather than finessed

These three cannot hold simultaneously:

```
A. Only PALW confers the right to produce a block.
B. Blocks are produced even when zero nodes can execute PALW.
C. There is no hash / PoS / BFT fallback production path.
```

If no party can execute PALW, there are no PALW proofs, and no rule can conjure a block from
their absence. ADR-0038 chose B and C by keeping a hash floor, which costs A. **This ADR chooses
A and C, and pays for it with B.** The guarantee therefore reads:

> While at least one genesis-authorized PALW class can produce certified work, the chain
> continues. When none can, the chain **halts loudly**. Validators do not manufacture economic
> blocks without PALW work.

That is a real cost and it should be signed for deliberately, not discovered later. The purpose
of Decision 1 is to make the antecedent overwhelmingly likely to hold.

## Decision 1 — The floor is a CLASS, not a hash: `PALW-BASE-0`

Mainnet genesis authorizes, alongside the accelerated classes, a portable base class:

* executes on **any** general-purpose CPU, scalar code path only;
* **no** GPU, CUDA, ROCm or Metal dependency;
* **no `libm` and no floating point** — fixed-point/integer arithmetic throughout;
* no external Ollama, no external llama.cpp, no external GGUF path: the model, tokenizer and
  kernel graph are pinned as a mainnet artifact;
* two independent implementations agreeing bit-for-bit before it may carry weight;
* **carries a non-zero share (≈ 5 %) in normal operation** and is exercised continuously;
* may receive up to 100 % of future share at an epoch boundary when other classes die.

The always-on requirement is the load-bearing half. A standby path that runs at 0 % share is a
path that has never run, and it will fail the first time it is needed — the failure mode is not
hypothetical, it is the ordinary history of untested failover.

Indicative shape, at most ~5 classes:

| Class | Role |
| --- | --- |
| CUDA | NVIDIA-class accelerated PALW |
| ROCm | AMD-class accelerated PALW |
| Metal | Apple-silicon PALW |
| CPU-OPT | AVX2 / AVX-512 / NEON |
| **CPU-BASE** | scalar integer implementation — always live, final survivor |

Degradation is then class redistribution, never a change of algorithm:

```
CUDA dies            → share to ROCm / Metal / CPU-*
all GPU classes die  → share to CPU-OPT / CPU-BASE
optimized CPU dies   → CPU-BASE alone, slower
```

**No hash block is produced at any step.**

### 1a. The Base class is built FIRST, not last — and this is the strongest argument for it

The natural reading of a "final survivor" is "the thing we finish last". That ordering is exactly
wrong here, for a reason the original proposal did not claim and which this ADR adopts as
normative:

> **`PALW-BASE-0` is the only class in which conviction is reachable in the near term.**

Integer-only means no `libm`, no FMA contraction, no transcendentals, no denormal behaviour — so
the ADR-0030–0033 kernel catalog can actually reach 100 % coverage for it. Every accelerated
class is blocked on transcribing quantized matmul, softmax and RoPE, which today leave the
catalog resolving 6 of 17 op kinds and a lie in the ops that carry the computation terminating
`Unadjudicable` (re-audit). Under optimistic verification, **conviction is the only thing standing
between a fabricated block and full weight**. Therefore:

* **No class may carry fork-choice weight until its catalog coverage is complete** (ADR-0038 A4's
  gate, made a weight precondition rather than only a credit precondition).
* `PALW-BASE-0` is the first class to satisfy it, so it is the first class that may carry weight,
  and it should be built and soaked first.

This also removes the `libm` false-conviction hazard from the floor entirely: an integer class
cannot have two honest implementations disagree by one ulp, so the failure that would slash an
honest producer (and, under the ramp, void weight retroactively) is unrepresentable there.

## Decision 2 — W6′ (supersedes W6): PALW-only liveness

> Mainnet shall not recognize hash-only work, hash-floor work, or any non-PALW block-production
> path. Every weighted block MUST carry a valid certificate for a genesis-authorized PALW class
> whose catalog coverage is complete.
>
> At least one portable, integer-only Base PALW class MUST remain Active with non-zero target
> share at all times.
>
> When an accelerated class becomes unavailable, its future epoch share SHALL be redistributed
> among other Active classes through finalized class-health transitions.
>
> If all accelerated classes are unavailable, the Base class MAY receive 100 % of future share,
> and the target block rate MAY decrease.
>
> If no genesis-authorized class can produce certified work, the network SHALL halt loudly.
> Validators MUST NOT manufacture economic blocks without PALW work.

### 2a. Degraded mode — what actually helps, stated correctly

Reducing target BPS in degraded mode is right, but **not for the reason usually given**: it does
not improve the security ratio. An attacker's advantage is their work over honest work, and
lowering the block rate scales both. Slowing production buys ordering stability and less work
wasted on orphans; it does not make a weakened chain harder to attack.

The levers that do change the ratio, or bound the damage:

* **suppress emission** in proportion to real work — do not pay a full subsidy for a fraction of
  the security;
* **extend the challenge window** — more wall-clock for a watcher to refute, against a smaller
  and slower adversary set;
* **extend or suspend finality for high-value operations** — bridge, mint, registry updates —
  while ordinary UTXO transfer continues;
* **raise the share threshold** at which a class may re-enter, so a flapping class cannot
  oscillate the domain.

Indicative only, to be measured, never shipped as-derived: `10 BPS → 2 BPS → 0.2–1 BPS`.

> **Throughput is the thing to spend in a degradation. Difficulty is not.**

## Decision 3 — W4′ (supersedes W4): two derived weights, one fork choice

### 3a. The constraint that decides the representation

The re-audit's first blocker is a mechanical fact, verified on this branch: a maturing weight
scalar **cannot** be `header.blue_work`. `blue_work` sits inside the pre-PoW preimage, is
miner-declared, and is validated by **exact equality** against the recomputed GHOSTDAG value
(`consensus/src/pipeline/header_processor/post_pow_validation.rs:49`); the pruning proof
(`processes/pruning_proof/{build,apply,validate}.rs`), difficulty and window machinery all read
it. A value that matures after the fact cannot be a value that is fixed under a PoW commitment
and re-derived identically by a pruning proof.

The resolution is not to make `blue_work` mutable. It is to **stop asking it to carry PALW
weight at all**:

* `header.blue_work` remains exactly what it is today — an immutable, PoW-covered accumulator —
  and pruning proofs, difficulty and window machinery are **unchanged**. Under Decision 4 it
  ceases to confer any production right; it is a structural accumulator, not a security budget.
* **PALW weight is never serialized into a header.** It is derived per node from DAG data, and
  the determinism obligation (W3) applies to the derivation.

### 3b. The two weights

```
safe weight(B)   = Σ pwu over MATURE ancestors only
                   (MATURE = receipt-licensed ∧ window closed ∧ no conviction ∧ no open dispute)

live weight(B)   = safe weight(frontier) + Σ bounded pwu over PUBLISHED, bonded,
                   not-yet-mature descendants of the frontier
```

* **safe weight** governs IBD, deep-reorg bounds and economic finality. A private fork cannot
  accumulate it, because it cannot obtain receipts from honest assigned verifiers while private —
  this is what kills fabricated forks, and it is why `ramp_stage_v1` was corrected (commit
  `04c2650`) so that a closed window without a receipt quorum does **not** finalize.
* **live weight** governs tip selection only, and only above the safe frontier. Its purpose is to
  stop assigned verifiers holding an honest chain hostage: withholding receipts must delay
  *maturity*, never *production*.

### 3c. One fork choice, not two

Two independently-consulted weights would be two answers to "what is the tip", which is a
partition waiting to happen. The rule is a single ordered procedure:

```
1. Take the highest safe-weight block — the SAFE FRONTIER.
2. Among its descendants only, select by live weight.
3. Ties break as today.
```

This is the familiar "finalized checkpoint, then fork choice above it" shape. Below the frontier
nothing is reorganizable by live weight; above it, live weight cannot outrank a matured branch.

### 3d. Determinism obligations (W3, restated as requirements on the derivation)

Every one of these is a partition if violated:

* No local wall-clock, no receipt **arrival** order, no peer-observation order may enter. Every
  threshold — receipt inclusion deadline, challenge deadline, weight activation, conviction
  cutoff — is evaluated at a **blue-score or finalized-epoch boundary**, never at "when I saw it".
* `live weight`'s bound on immature pwu must be a fixed fraction fixed at registration, so two
  nodes that disagree about nothing cannot disagree about the bound.
* A pruned node must reach the same answer as an archival node. Facts the derivation needs must
  live in pruning-surviving state, or the derivation must be defined to a bounded horizon that
  fits inside the pruning horizon. **Reading absent data as "nothing" is forbidden** — a missing
  fact is an error, never a permissive zero.
* GHOSTDAG's k-cluster blue-set computation continues to use `blue_work`; it is about DAG
  structure, not work magnitude. Only tip/virtual selection consults PALW weight. Affected site
  to change deliberately: the header-selected-tip store's ordering
  (`header_processor/processor.rs:416`).

### 3e. Retroactive void, bounded

A conviction before maturity voids that block's pwu (W5, unchanged). Because safe weight counts
only MATURE work, a conviction can never rewrite safe weight — it can only prevent work from
entering it. Retroactive void therefore acts on live weight alone, inside the challenge window,
above the safe frontier. This is what keeps "mutable weight" from meaning "mutable history".

## Decision 4 — The ticket is not a hash puzzle

The criterion, adopted verbatim as the test any future ticket construction must pass:

> **Does trying one new ticket require one new PALW execution?**

`H(header) < target` fails it: a candidate costs a hash. The construction landed on this branch
(commit `b385803`) passes it, by binding the tag to the exact attempt —
`challenge = H(network ‖ pre_pow_hash ‖ timestamp ‖ nonce ‖ class ‖ bond_outpoint)`, with the
tag derived from the trace under that challenge. Changing the nonce changes the challenge and
owes a new inference; a commitment cannot be replayed onto another attempt, header, class or
executor.

Two refinements the ADR fixes normatively:

**4a. A final hash is mandatory, and does not make this hash-based production.** Deriving the
ticket from `H(domain ‖ trace_accumulator ‖ challenge ‖ class_id)` is not a hash lottery, because
`H` is an *extractor* over a value that costs an inference to produce: grinding `H` a trillion
times yields one candidate, since the accumulator does not change. Hashing is what makes the
ticket uniform; the inference is what makes it scarce.

**4b. A bare linear accumulator MUST NOT be the ticket.** A rolling accumulator of the form
`A_{j+1} = α_j·A_j + β_j·Q(S_j) mod p` makes `A_m` a **linear combination** of the per-step
states. Any freedom in a late `S_j` — a quantization boundary, a controllable trailing token,
padding — lets an attacker *solve a linear equation* for a value landing `A_m` under target,
with no re-execution. That defeats the Decision 4 criterion directly. The accumulator may be a
useful internal digest; it may not be the ticket without a final cryptographic hash over it.

## Decision 5 — Per-class share, epoch caps, and how caps are enforced

Static cross-class PWU coefficient tables are rejected (ADR-0038 Decision D, retained): any
mispriced class becomes a standing arbitrage and multi-class collapses into a monoculture, which
is the failure the multi-class design exists to prevent. Price is discovered per class by
independent DAA; PWU stays static **within** a class, derived from normative operation counts.

To stop a transiently mis-tuned DAA from letting one class flood the DAG, each class carries an
epoch weight budget:

```
Σ_{b ∈ class c, epoch e} weight(b)  ≤  s_c(e) · W_e
```

**The cap is enforced at admission, by rejection — not by zeroing weight afterwards.** Zeroing
would add a second way for a block's weight to change after acceptance, which is the exact class
of mutation Decision 3 exists to bound. A block that would exceed its class's epoch budget is
not a low-weight block; it is not a block.

Share farming is bounded by the same registration gate as everything else: a new class may not
receive share until its catalog coverage is complete and its two independent implementations
agree, and it enters at a share set by finalized class-health transition rather than by
self-declaration. A class whose producers all leave stalls its own domain and loses share at the
next finalized boundary; its share is redistributed, never stranded.

## Decision 6 — Bonded is not permissioned

ADR-0038's phrasing that "pure permissionless PALW mining is not achievable" is withdrawn as
misleading. A bond that anyone may lock by protocol is **bonded permissionless mining**: the bond
is collateral that makes a false commitment expensive, not a licence that someone grants.

What remains prohibited, and what a reviewer should check for: administrator approval,
allowlists, manual registry vetting, or any validator holding a veto over who may produce.

Relay may bound in-flight candidates per bond outpoint (one, or a small fixed number, of
immature candidates). That is what replaces the hash anti-spam floor for flood control, and it is
an identity-neutral rule.

## What this ADR does not decide

* **The PWU derivation.** It carries 90–99 % of weight and is today a self-declared `u64` checked
  only for non-zero (re-audit blocker 6). Its normative derivation is required before any class
  carries weight, and is its own decision record.
* **Catalog completion** for the accelerated classes, and the second implementations.
* **Mainnet parameters** — identity, genesis, window set, share table, `ρ_r`, `k`, bonds. Those
  are soak outputs (ADR-0036), and this ADR deliberately fixes only shapes and inequalities.
* **Whether the accumulator of 4b is used at all internally.** Only its use as a bare ticket is
  refused.

## Consequences

* **A required adversarial suite before any value network.** The theorem to prove is:
  *equal DAGs ⟹ equal weights*, invariant under receipt arrival order, conviction observation
  order, pruning point, and IBD start height. ADR-0038 named mutable-weight fork choice as its
  own hardest correctness target; this makes that a gate rather than an intention.
* **Code sites this ADR commits to touching:** the header-selected-tip ordering
  (`header_processor/processor.rs:416`), virtual/tip selection in the virtual processor, and a
  new derivation module for the two weights. `blue_work`, pruning proof, difficulty and window
  machinery are deliberately **not** touched.
* **`spam_hash_work` leaves `weight(B)`.** With the term removed, an unlicensed block's weight is
  its bounded live pwu, not a hash quantity — and W4′ replaces W4's "never exceeds spam-hash
  backbone influence", which is meaningless once there is no backbone.
* **The Base class becomes a build-order commitment**, not only a design one: it is the first
  class to complete, the first to carry weight, and the reference against which the accelerated
  classes' catalogs are judged.
* **The halt is real.** Operators must be told plainly that a total PALW outage stops the chain,
  and that this is chosen. The alternative — a hash path that can produce economic blocks — is the
  thing the whole design refuses.

## Residual assumptions (what signing this accepts)

1. At least one Base-class operator is reachable and honest within every challenge window.
2. Two independent Base implementations can be built and kept bit-identical — the integer-only
   constraint is what makes this tractable.
3. Catalog coverage is achievable to 100 % for the Base class's op set.
4. The bonded producer set is large enough that "bonded permissionless" is not permissioned in
   practice. Today it is four hosts under one administrator; that is a launch precondition, not a
   property of the design.
5. Class-health transitions are observed identically by every node, because they are finalized
   facts — a class-death judgment that differs between nodes is a partition.
