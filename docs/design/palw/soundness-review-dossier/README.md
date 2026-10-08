# PALW soundness review dossier — RFC-0007 Part V/VI and RFC-0015 (OPV)

Prepared by agent SOUND (branch `sound/review-dossier`, base `b676927de`), 2026-10-08/09, for an **external** soundness review. The
review itself is an external gate: nothing here approves a policy, arms a fence or claims a property the code does not establish.
The dossier states the claims precisely enough to be checked, names every assumption they rest on, lists every parameter with its
source, records every known gap, and gives the composition into a whole-claim bound with worked numbers that a test pins.

## Contents

| File | What |
|---|---|
| [01-statements.md](01-statements.md) | The statements under review, formally, with code references: S1 single-modulus Freivalds; S2 multi-modulus CRT for `i128`; S3 exact recompute, state continuity, the media-pipeline edge, and the coverage lemma L-COV; S4 K2S's element courts and the first-wrong-value induction; S5 sampled conformance; S6 the PALW Work Beacon, the seed and the samplers; S7 OPV window, economics and liability; S8 OPV-BOOT's effective-bits accounting |
| [02-assumptions.md](02-assumptions.md) | Hash / ROM / QROM, semantics, the honest-verifier liveness assumption, beacon sources, synchrony, economic rationality, chain |
| [03-parameters.md](03-parameters.md) | Every current value with `file:line`, marked INTERIM / DESIGN / CONSTANT / TEST-ONLY; the user's targets; the formulas the parameters must satisfy |
| [04-gaps-and-findings.md](04-gaps-and-findings.md) | Last-contributor grinding and the sealed-source beacon v3, the complete-check bootstrap, F-C4R3-*, the interim 2-bit conformance, and this dossier's own findings, each with status at the base and where fixed |
| [05-composition.md](05-composition.md) | How per-relation soundness composes over a claim and over the beacon; the bound formula; worked 9B-8k numbers (rows A–M) |
| [06-reviewer-questions.md](06-reviewer-questions.md) | The open questions, ranked by risk |
| [07-reproducibility.md](07-reproducibility.md) | How to run the golden vectors, the kernel tests, lane D's `g14_*` and this dossier's checks |

Executable half: `misaka-palw-challenge/src/composition.rs` (a pure integer calculator; no consensus or policy code calls it) and
`misaka-palw-challenge/tests/composition.rs` (rows A–M pinned, plus toy-field Freivalds and CRT experiments with the crate's own
sampler). No consensus behaviour, wire id, fence, parameter or golden vector of the protocol changes.

## The argument in one page

1. **Per relation** (S1, S2): a committed false matrix product passes `t` independent post-commit Freivalds checks with probability
   at most `q^-t` for the modulus `q` in which the integer error survives; K2-TIR-v1 uses `2^127 − 1` (126 bits per repetition) and
   refuses spans that could alias; K2-TIR-v2 checks modulo the fewest of `2^127 − 1, 2^107 − 1, 2^89 − 1` whose product exceeds the
   span (88 bits per repetition, worst case). Exact families (S3) have error 0 given binding.
2. **Per claim** (L-COV, §5.3): the plan checker makes coverage code-derived, so a false claim has a false instance; the kernel
   composes by a union over instances and the binding term. At 9B-8k: **`2^-226` (v1), `2^-150` (v2)** with a uniform seed and
   complete coverage.
3. **Over the beacon** (§5.3): an adversary steering among `G` outputs per beacon, with retries and many statements, multiplies the
   error by `Q·(R_a + 1)·G^β`. **Today's beacon has unbounded last-contributor grinding** (`2^40` offline hashes, 2 retries and `2^20` statements leave v2 at 88 bits); the
   sealed-source beacon v3 is designed, not built; its honest-inclusion property is the top beacon question.
4. **Who checks** (§5.4): no repetition repairs selection. A verifier that checks 8 of 8,192 positions of a 9B-8k claim gives no
   bits; a whole-claim check reads ≈ 12.6–16 TB. Under OPV, protection then rests on deterrence, at a reservation of
   `gain · P/m / (1 − a)` (40,960 BILI at the interim gain, against the interim 1,000 BILI).
5. **What is implemented as soundness-bearing today**: outsiders' private-salt checks (no beacon), exact courts, the OPV clock and
   reservation rules. No public-coin per-claim check is wired; sampled conformance runs at an interim 2 bits with a vacuous fault model.

## Top reviewer questions (full list: [06](06-reviewer-questions.md))

1. **Q-01** — Is complete detection coverage at real scale tenable (12.6–16 TB per 9B-8k claim), and if not, is deterrence with a
   derived detection probability an acceptable security argument, or must a sublinear-read proof precede OPV for such classes?
2. **Q-02** — For the sealed-source beacon v3: is `G ≤ 2^a·F` the complete lever set, and what bounds `ε_src` when an adversary can
   seal the first `k` positions or censor honest seals?
3. **Q-03** — Does the PALW Work Beacon's security rest on computation cost at all, or is it a bonded commit–reveal beacon?
4. **Q-04** — Is the union over instances (and descriptor-wide 88-bit pricing) the policy's accounting, and is the tighter
   single-false-instance accounting valid? (It matters only while `t = 2`.)
5. **Q-05** — Is the multi-modulus CRT relation sound as implemented (span bound, range checks, moduli choice, independent vectors,
   reduction)?

## Conventions

* `file:line` references are at `b676927de` unless a branch is named; design documents on branches are cited as
  `branch:path` with their commit.
* "bits" means `−log2` of an error probability; "effective" means after retries, grinding, multiple relations and statements.
* The fork-choice safety analysis is internal (agent FINX) and is referenced only by that name; this dossier describes no fork-choice
  mechanism.
* Amounts are BILI (ADR-0174; `SOMPI_PER_KASPA` is the legacy name of 1 BILI = 10^8 sompi).
