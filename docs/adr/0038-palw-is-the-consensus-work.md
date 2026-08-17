# ADR-0038: PALW is the consensus work — sampled-verified LLM PoW, a receipt-licensed weight ramp, and a hash anti-stall floor

Status: **Accepted (architecture decision).** Supersedes **ADR-0037 Decision 1** (the layer
inversion below) while carrying forward ADR-0037's machinery (Decisions 2–9) re-seated under the
new layer assignment. Everything else in ADR-0036 (lineage, new-identity requirement,
land→accept→mint separation, no per-job override) stands.

Date: 2026-08-17
Relates to: ADR-0021 (algo-4 — its *lottery shape* is kept, its *verification shape* is replaced),
ADR-0026/0027/0028 (court + fraud proofs + sampling — promoted from credit machinery to L1
machinery), ADR-0030–0033 (the arithmetic court — unchanged), ADR-0034 (classes/bands — promoted
to consensus difficulty domains), ADR-0037 (the state machine and mint hygiene — carried,
re-seated), docs/palw-mainnet-readiness-audit (the 9 blockers — dispositioned in §Audit closure).

## Context — why ADR-0037 Decision 1 was wrong

ADR-0037 made hash-PoW the primary consensus work and PALW an asynchronous reward subsystem.
That resolves every liveness blocker — by abandoning the point. If hash is the security resource,
LLM compute is a subsidy program: the chain is exactly as secure with PALW switched off, miners
treat inference as an optional side business, and "useful AI compute as Sybil resistance" is a
marketing sentence, not a mechanism. The project's thesis requires the scarce resource that
orders blocks to BE the inference.

The audit's actual finding was narrower than "PALW cannot be L1". It was: **the current
implementation makes every full node execute the same giant LLM runtime to validate any block,
and panics when that runtime is absent.** The fatal coupling is in *verification*, not in
*production*. The fix is to change who verifies and how — not to demote the work.

What the current `algo_id = 4` already gets right (measured, `consensus/pow/src/lib.rs`):
the lottery shape. `seed = (network_id, pre_pow_hash, timestamp, nonce)`; one deterministic
pinned-LLM inference per nonce; the 200-byte tag feeds the BLAKE2b-512 finalizer; the digest
compares against `bits`. That is already "one canonical inference = one lottery ticket,
header-bound, non-transferable, progress-free". It is kept verbatim. What is replaced is the
verifier: full nodes stop re-running inference, ever.

## Decision A — The layer inversion

```
MISAKA value network
  PALW  = the consensus work (~90–99% of effective weight in normal operation)
  Hash  = spam ticket, header binding, randomness, tie-break,
          and the anti-stall floor (~1–10%; sole survivor only in catastrophe)
```

Block production requires:

```
valid_palw_block =
    valid_header
  ∧ spam_hash(header, nonce) < spam_target            (cheap, always CPU-checkable)
  ∧ header carries palw_commitment_root                (trace/output Merkle root)
  ∧ executor references an Active bond of an Active ExecutionClass
  ∧ palw_ticket < class_target                         (the algo-4 lottery, kept)
  ∧ well-formed carriage (sizes, class id, signature length)
```

None of these require any full node to run an LLM. `palw_ticket` is *checked* against the
committed root and header binding; whether the root is an honest inference is decided by
Decision C's sampled verification and court, under Decision B's weight ramp. A full node's
validation surface is: hashes, Merkle roots, signatures, and (in disputes) one bounded CPU
primitive.

**Bonded producers are mandatory.** This is not optional decoration; §New-risk 1 shows the
design is unsound without it. A PALW block names its executor's `bond_outpoint`; conviction
slashes it. Permissionless entry = post a bond, exactly the existing 20k-class bond flow.

## Decision B — Acceptance now, weight later: the ramp

Block acceptance and PALW finality are split. An admitted block enters the DAG immediately;
its *weight* matures:

```
weight(B) = spam_hash_work(B)                                  (the backbone, always)
          + pwu(B) × ramp(B)

ramp(B) = 0        at admission                     (provisional)
        = ρ_r      once ≥ k assigned receipts land  (receipt-licensed; ρ_r ≈ 1)
        = 1        at PALW-final
        = 0        forever, if convicted before PALW-final (retroactive void)

PALW-final(B) ⟺ challenge window W_challenge passed with no surviving refutation
```

* After PALW-final, weight is immutable — conviction can no longer void it (finality means
  finality; the window is sized so that cannot happen against a live watcher, §Assumptions).
* Fork choice (GHOSTDAG blue work) reads `weight(·)`; reorg semantics are unchanged, only the
  work scalar matures over a bounded horizon. Weight mutation is **monotone within a branch
  except the single conviction edge**, and every mutation is recomputable from DAG data alone —
  two nodes with the same DAG compute the same weights (ADR-0037 I14 carried).
* **DAG-native receipts:** verification receipts are ordinary carriage riding *successor*
  blocks (`verify(B, sample #i)` in C, D, …), so verification is chain activity, receipt
  inclusion is fee-earning, and the ramp is computable from the DAG with no side channel.

The honest cost, stated: full-weight finality lags by `W_challenge`. `W_challenge`,
BlockDAG finality, payout finality and block interval are four independent parameters
(ADR-0037 Decision 10 carried); 10 BPS constrains none of them structurally, because
verification is off the admission path.

## Decision C — Verification: assigned sampling is the alarm, the court is the truth

* **Assigned, mandatory sampling.** The ADR-0028 lottery (`select_replay_panel_v1`, future
  anchor, ADR-0037 Decision 4 inputs) assigns a per-block panel from the block's class.
  Assigned duty is objective: attest-or-no-show, no-show is an offense (bond-relevant).
  Panels sample positions derived from post-anchor randomness (layer/token/GEMM coordinates),
  recompute, and file bonded receipts. **Receipts are claims, not truth** — an attester who
  receipts a sample the court later convicts is convictable on their own signed roots
  (rubber-stamping is a bonded bet, this is the P_check answer carried from ADR-0037
  Decision 8).
* **Permissionless refutation.** Anyone bonded may file a refutation regardless of panel
  membership (ADR-0027 unchanged). The security statement of the fast path is 1-of-N honest
  *watcher*, not q-of-n honest *committee*: quorum accelerates the ramp; it never becomes
  final truth.
* **The court is unchanged** (ADR-0030–0033): checkpoint → layer → step-leg → kernel →
  primitive; the full node re-executes ONE catalogued primitive on CPU (vendored
  SoftFloat / fixed integer semantics) against Merkle-proven operands. GPU-less VPS
  adjudication, exact bits, three verdicts. `Unadjudicable` semantics (ADR-0037 Decision 2:
  no slash, class freeze) now apply to *block work*: the block's pwu voids (ramp → 0 if not
  yet final), nobody is slashed, the class freezes.

## Decision D — Multi-class difficulty: per-class DAA, static PWU only inside a class

The class taxonomy (ADR-0034) is promoted from a routing concern to a **consensus difficulty
domain**, and difficulty is measured in PALW Work Units:

* **Intra-class:** `pwu(class)` is a static, canonical work score derived from normative
  FLOPs, memory traffic, token count, context length and quantization class — never measured
  wall-clock (an RTX 5090 second and a 2019-GPU second are different amounts of nothing).
  The derivation is registered with the class and frozen.
* **Inter-class:** class ratios are NOT hand-priced. Every hand-set cross-class coefficient
  is a standing arbitrage: the mispriced-cheap class absorbs all miners and the multi-class
  resilience this ADR buys dies as monoculture. Instead **each Active class runs its own DAA**
  (the multi-algo-chain construction — Myriad/DigiByte-style — with per-class targets), each
  targeting its share of block cadence. Real miner economics then price the classes
  continuously, and no committee maintains a coefficient table.
* **Class failure = redistribution, not halt.**

```
class dies (runtime bug, weights unavailable, Unadjudicable, freeze)
      → its share redistributes across surviving Active classes (their DAAs absorb it)
      → chain cadence recovers at the next adjustment
all classes dead
      → hash anti-stall floor: the spam-hash backbone alone produces (slow, degraded,
        near-zero-subsidy) blocks — the chain limps, visibly, and never halts (I2')
```

The anti-stall floor is deliberately unattractive (tiny share of subsidy, slow cadence): it
exists so "every class dead" is an incident, not an extinction — and so no one can profitably
mine it while any class lives.

## Decision E — What hash still does

1. **Spam ticket:** `spam_hash < spam_target`, cheap but nonzero — a candidate PALW block
   costs something CPU-objective before anyone evaluates its carriage. This bounds
   garbage-candidate flooding (the "million fake candidates per second" problem).
2. **Header binding & randomness:** the finalizer construction is kept; post-anchor
   randomness for sample positions extracts from DAG hashes.
3. **Anti-stall floor** (Decision D): degraded-mode production.
4. **Tie-break** in fork choice, as today.

Hash is never again the primary security budget on a value network.

## Decision F — Fork choice, IBD, and the fabrication problem

PALW work is **not self-authenticating** — that is the deep difference from hash-PoW and the
one place this design must not hand-wave. A fabricated commitment root costs zero GPU work;
only verification distinguishes it from real work. Therefore:

* **Weight is receipt-licensed or finality-licensed, never claim-licensed.** A block's pwu
  enters fork-choice weight only at `ramp ≥ ρ_r` (bonded receipts from the assigned panel —
  ML-DSA signatures a private-fork attacker cannot forge for validators they don't control)
  or at PALW-final *on the receipt-covered chain*. An attacker's private fork can fabricate
  its own blocks but not the panel receipts of bonded validators it doesn't own, so its
  fabricated pwu never matures; its fork weighs `spam_hash_work` — the deliberately tiny
  backbone.
* **IBD:** a syncing node validates headers, spam hashes, carriage shape, receipt coverage
  and refutation absence — all DAG-objective; it never re-runs inference. Deep history is
  additionally fenced by the existing pruning-depth rule and the VLT finality overlay
  (ADR-0009/0024 lineage), which was built for exactly this class of deep-reorg refusal.
* Within the unbonding period, receipt keys are slashable; the long-range variant (keys that
  already unbonded re-signing an alternate past) is fenced by pruning depth + VLT, the same
  two fences the chain already relies on for stake.

## Decision G — What survives from ADR-0037, re-seated

| ADR-0037 piece | Fate here |
| --- | --- |
| Job state machine (`palw_job_state.rs`, I8/I9/I10 as theorems) | **Carried.** The lifecycle now describes a *block's PALW work*: Provisional → ChallengeWindow → Disputed → court verdicts. The landed module is the spine unchanged; `Open/Committed/PanelSelected` map to production-side states. |
| Court (ADR-0030–0033) | Carried verbatim; promoted to L1. |
| Identity/signature binding (Decision 3 domains) | Carried; commit message binds the header context. |
| Future-anchor panels, dual deadlines (Decision 4) | Carried; now assigns *block* panels. |
| Class registry, freeze, six-path gate (Decision 9) | Carried; freeze now also removes the class from the difficulty domain set (Decision D). |
| Mint hygiene: subsidy carve, budgets, exact bond-outpoint payees, three-pool separation (Decision 7) | Carried for *rewards*. Block subsidy splits across class-share + receipt fees + anti-stall floor share; still never exceeds schedule (I6/I15). |
| P_check exclusion, leverage caps (Decision 8) | Carried; receipts are bonded bets (Decision C). |
| No admin per-job/per-block override | Carried (I13). |
| **Decision 1 (hash floor as primary; PALW never block-critical)** | **Superseded.** Inverted by Decisions A–E. |
| M0–M3 staging | Reshaped: M-stages now stage *class count, receipt quorum k, ρ_r, and the PALW share of subsidy*, not "PALW off→on". A soak network runs the full shape from its genesis. |

## Invariants v2 (release-blocking; supersede ADR-0037's I1/I2/I12, carry the rest)

```
W1  A full node validates every block, and adjudicates every dispute, with no LLM
    runtime, no GPU, and no model artifacts beyond Merkle-proven operands
W2  One PALW ticket costs one canonical inference, bound to (network, header, nonce);
    tickets are non-transferable and non-replayable across headers
W3  weight(B) is a pure function of the DAG; equal DAGs ⟹ equal weights everywhere
W4  Unverified (receipt-less, non-final) pwu never exceeds spam-hash backbone influence
    in fork choice
W5  PALW-final weight is immutable; pre-final conviction voids exactly the convicted
    block's pwu, nothing else
W6  Chain produces blocks while ≥1 ExecutionClass is Active; with zero, the anti-stall
    floor produces degraded blocks; no input whatsoever halts the chain or panics a node
W7  Class freeze removes a difficulty domain and redistributes cadence; it never
    invalidates already-final weight
W8  Producer and attester accountability is bonded: no bond, no block; no bond, no receipt
I3–I11, I13–I15 of ADR-0037 carry unchanged (credit-once, exact payees, full binding,
budget ceilings, missing≠empty, refutation-locks, conviction-only slash, no-Unadjudicable
slash, no cross-class slash, no overrides, deterministic state, emission schedule)
```

## Audit closure — do the criticals recur?

Disposition of the 2026-08-16/17 blocker classes under this design:

| Blocker class (audit) | Disposition |
| --- | --- |
| Every full node depends on one giant runtime; `panic!` on `PalwUnavailable`/`PalwWorkerFailed` | **Eliminated by construction** (W1, W6): full nodes never invoke a runtime. The panic sites die with the re-verification path itself, not by wrapping them. |
| Exclusive `algo_id = 4` = single point of chain death | **Eliminated** (Decision D): N Active classes are N independent production paths + the anti-stall floor. Note: NOT by demoting PALW — by multiplying classes. |
| 100 ms block interval physically incompatible with 37–91 s replay | **Eliminated** (Decision B): replay is off the admission path; it constrains `W_challenge` only. The `finality_depth < W_challenge` preset failure becomes a parameter re-derivation, not a wall. |
| Consumer layer fail-open ×10 (lookup collisions, missing-as-empty, unverified carriage at credit entry) | **Not addressed by this ADR's shape — addressed by the carried Track-C machinery** (ledger state instead of horizon re-walks, verified-entry types, I5/I7). These blockers were never about the layer assignment and would recur in ANY design if Track C is skipped. |
| Payee by `validator_pubkey_hash`; unbounded coinbase append | Carried fix (Decision G: exact bond-outpoint payees, budgeted batch — I4/I6). |
| Artifact pinning (B8 libm, B15 GGUF CWD bypass) | Landed 2026-08-17, carried. Full nodes need it never; *miners and attesters* need it for class identity. |
| Gate-ledger through-line false | Process rule carried from ADR-0036 Decision 3. |

**Verdict: none of the audited criticals recurs — two of the seven classes are closed by this
ADR's structure, and the rest are closed by machinery this ADR carries forward. But the design
introduces three NEW critical-class risks, closed as follows; a review that only re-checks the
old nine would miss them.**

## New risks introduced by PALW-as-L1, and their closures

1. **Fabrication economics (the killer).** With optimistic default-accept, a fake commitment
   root costs ~0 and, unrefuted, would finalize: fabricated pwu for free, repeated forever.
   *Closure:* bonded producers (Decision A) + mandatory assigned sampling with near-certain
   coverage (Decision C) + receipt-licensed weight (W4). Expected value of fabrication is
   `−bond × P(conviction)` with `P(sample) ≈ 1` by assignment; unsampled fabricated work never
   outweighs the backbone. **Without bonds this design is unsound; "pure permissionless
   PALW mining" is not on the table and the ADR says so explicitly.**
2. **Mutable-weight fork choice.** A weight that changes after admission is a new consensus
   surface (the audit taught what those cost). *Closure:* W3 (weight is a DAG-pure function),
   W5 (single, monotone-except-conviction mutation, bounded horizon `W_challenge`), and the
   rule that PALW-final weight is immutable. This is the change set's hardest correctness
   target and gets its own adversarial suite before any value network (reorg-equivalence
   tests: same DAG ⟹ same virtual, across receipt/conviction orderings).
3. **Class monoculture via mispricing.** *Closure:* per-class DAA (Decision D) — prices are
   discovered, not maintained. Residual: a class whose real cost collapses (new hardware)
   gains share until its DAA catches up — bounded by adjustment latency, same exposure every
   multi-algo chain carries.

## Residual assumption set (what a signer of this ADR accepts)

```
A1  ≥1 honest, bonded watcher per Active class is live within every W_challenge
    (1-of-N optimistic assumption; panels make coverage assigned, not hoped-for)
A2  The bonded receipt set is not majority-corrupt within the unbonding period
    (deep-reorg fenced by pruning depth + VLT beyond it)
A3  Class PWU derivations are canonical and frozen; per-class DAA absorbs pricing error
A4  The court's kernel catalog is 100% of reachable kernels per Active class
    (Track D gate unchanged — Unadjudicable-on-gap + freeze is the enforcement)
A5  Spam target is low enough to be a ticket, high enough to make candidate flooding
    and anti-stall-floor takeover uneconomic
```

## What this ADR does not decide

Numeric parameters: spam_target share, ρ_r, k, W_challenge, per-class initial targets, the
PWU derivation formula, anti-stall subsidy share, bond sizes. All are soak/simulation outputs
(ADR-0036 "does not decide" carried). The TN11/devnet migration path (they already run the
single-class ancestor of this shape; the delta is commitment-root headers + receipts + ramp)
is a soak-planning decision. M3 (PALW weight in VLT/BFT voting) remains a separate ADR.

## Summary

ADR-0037 secured the chain by making PALW optional. That was the wrong fix for the right
blockers. The right fix keeps inference as the scarce resource that orders blocks — the
algo-4 lottery already had the correct shape — and replaces the one fatal coupling: full
nodes verifying by re-execution. Verification becomes: assigned bonded sampling as the alarm,
the existing exact-bit court as the truth, weight that matures with evidence, classes that
fail independently, and a hash floor demoted from "the security" to "the reason the lights
never go fully out".
