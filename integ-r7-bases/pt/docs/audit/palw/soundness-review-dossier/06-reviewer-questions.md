# 6. Reviewer questions, ranked by risk

Risk = (how much of the security argument fails if the answer is unfavourable) × (how uncertain the answer is today). Each question
names the statement, assumption and finding it bears on, and what evidence would settle it.

## Critical

**Q-01 — Is complete detection coverage at real scale a tenable assumption, and if not, what replaces it?** (A-HV, S4, §5.4,
SG-06.) A whole-claim check of 9B-8k reads ≈ 12.6 TB (measured, K2-TIR-v2) / ≈ 16 TB (v4 estimate); K2S v4 bounds only the
*post-detection* prosecution (two positions, ≈ 7.5 GB). A verifier sampling `m` of `P` positions gives `⌊log2(P/(P−m))⌋` bits — 0
for 8 of 8,192 — and leaves only deterrence at `gain·P/m/(1 − a)`. Please assess: (a) whether OPV for classes whose whole-claim
material exceeds what one verifier reads within `B_cold + B_check` can be called sound at all; (b) whether a segment-assigned or
watcher-market coverage with a *derived* `P_dc` suffices; (c) whether a sublinear-read proof (GKR/sum-check with a polynomial
commitment, RFC-0007 §V.4) must precede activation for such classes. Evidence: MEAS lane's fresh-verifier timings per class; a
coverage model with its own failure probability.

**Q-02 — What is `G`, and what is `ε_src`, for the sealed-source beacon v3?** (S6.3, A-B2, A-B3, SG-01, SG-01a.) Today's accumulator
has unbounded offline last-contributor grinding (row D: 88 bits). For v3 (sources ordered by bonded seal position, sealed in the window,
revealed after): (a) is `G ≤ 2^a · F` (subset withholding of the adversary's `a` sealed sources × reachable branches) the complete
lever set, including reveal timing and pre-Final abandonment of a revealed source; (b) if the selection is "the first `k` seals", what
prevents an adversary that seals the first `k` positions (or censors honest seals) from controlling every source and grinding their
contents before sealing — i.e. what bounds `ε_src`; (c) is a 1-BILI seal deposit a meaningful price per withheld bit. Evidence: the
v3 specification and C4 round 4's grinding attacks.

## High

**Q-03 — Does the beacon's security rest on computation cost at all?** (A-B6, SG-02.) RFC-0007 §VI.3 requires that an alternative
entropy sample cost additional paid useful computation. A complete-check bootstrap class's work is ≤ `2^26` units and deterministic;
any class whose job the adversary posts yields a new sample for a job fee and a seal deposit. If the answer is "no", the PALW Work
Beacon is a bonded commit–reveal beacon and should be analysed as one (bias = withholding; cost = deposits), with the useful-work
framing dropped from its security claim.

**Q-04 — Which accounting is the policy's: union over instances and descriptor-wide moduli, or tighter?** (S1–S3, §5.3, SG-08.)
For commit-then-challenge, non-aggregated Freivalds, a false claim must pass the check of a fixed false instance, so `⌈log2 R⌉` is not
needed (row H: 175 vs 150 bits at 9B-8k); K2-TIR-v2 prices every repetition at the 88-bit modulus even for relations using only
`2^127 − 1`. Please confirm the conservative accounting is valid, and state whether the tighter one is (it matters only if `t` stays 2).

**Q-05 — Is the multi-modulus (CRT) relation sound as implemented?** (S2.) Check: the span bound `max|out| + k·max|a|·max|b|` and its
`+2`-bit margin (`plan.rs:233–243`); that every opened output is range-checked (`verify.rs:416`) so `|E| ≤ span`; that the moduli are
distinct primes and the fewest-moduli rule (`dense_moduli_v2`) never under-provisions (`∏(2^{e_i} − 1) > 2^{Σe − 1}`); independent
vectors per `(repetition, modulus)` (`verify.rs:838`); the `i128` field mapping including `i128::MIN` (`field.rs:91`); and the
reduction routine `reduce_wide` for every `e ∈ {127, 107, 89}` (`field.rs:61`).

**Q-06 — What fault model can sampled conformance claim?** (S5, A-FAULT, SG-03, SG-15.) The bound `(1 − f)^n` is conditional on a
fault density `f`; scope v1's default `(1/2, 1/16)` has no review, and the only approved model (`f = 1`) is vacuous. Implementation
faults are often sparse (a dtype path, a rare shape, a trigger). Please assess whether any `f` is defensible, whether conformance
should be stated purely as "implementation agreement on a uniform sample" with no fidelity claim, and confirm that no part of the
per-claim argument relies on it (the dossier's composition does not).

**Q-07 — Is the OPV deterrence inequality complete?** (S7.2, A-ECON, SG-13, SG-14.) `reservation · (1 − a) · P_dc ≥ gain` with
`P_dc` *assumed* (500 ‰ interim). Please assess: deriving `P_dc` from coverage (§5.4) and from enforcement failures (`ε_enf`);
whether `external_gain_bound` (fork-choice influence, external settlement) can be bounded; correlated failure (no watcher for many
concurrent lies) under the live caps; self-conviction recoup after the accuser seal (G14-R4); and the withholding path (default
penalty only, no Final).

**Q-08 — Does the first-wrong-value induction hold for every v4 primitive?** (S4b.) The four preconditions of §S4: `(p, s, n)` is a
topological order of the whole dependency graph including `p − 1` reads and the full-window shift; each primitive's leaf set covers its
dependency line; the reduced court evaluation equals the full semantics at one element (exact-result rule, `Cast`, `Div`, `Gather`);
the decode tie rule. Also: dual-root (row/column) commitments that disagree are convicted as inconsistent — confirm no honest
producer can be caught by it.

## Medium

**Q-09 — Grinding for interactive suites.** (S6.6, S8, row M.) Is `G^β` the right charge for a staged beacon, or can a round-by-round
argument give `≈ G·β·max_j ε_j`? For the transcript-bound Fiat–Shamir mode: QROM security of the transform as specified
(`seed.rs:211–262`), adaptive grinding of prover messages, and the consumer-side statement check (GAP-C4-B).

**Q-10 — OPV timing relations.** (S7.1, A-NET, A-BUDGET.) Are the five relations of `OpvPolicyV1::validate` sufficient — e.g. for a
demand opened in the window's last block, a chunked response, a reorg between a served demand and the proof — and what inclusion
assumption against censorship is the production window allowed to rest on (`censorship_cost` is a lower bound; C4 round 3 found
cheaper paths, now fixed on `g14/r4-fixes`)?

**Q-11 — Quantum adversaries.** (A-H2, A-H4.) The binding term (256 bits classical; ≈ 170-bit generic quantum collision bound) and
quantum search over a steerable seed space (quadratic advantage in finding a bad seed). Which figures should the production effective
bound use?

**Q-12 — One seed derivation or two?** (SG-07.) The contract's `challenge_seed_v1` is meant to feed the kernel's own
`ChallengeBindingV1::seed()` and stream. Confirm the composition is sound under A-H1, that the kernel stream's labels are injective
(`challenge.rs:69–87`), and decide whether the kernel stream needs golden vectors and a bound of its own.

**Q-13 — Predictable-coin entry points.** (SG-05.) `FreshVerifierV1::check` uses the public record's `beacon = [0; 64]`. Confirm it is
never soundness-bearing (only tests call it today) and decide whether it should be removed from any seat-facing path.

## Low

**Q-14 — Samplers and encodings.** (S6.4, A-H3.) Exact uniformity of `index_below`, Floyd's `distinct_indices`, `field_m127`, the
kernel's `next_in`; exhaustion semantics (`2^24` words; SG-09); Borsh injectivity of every seed-bearing type; domain separation of
keyed BLAKE2b with keys of different lengths.

**Q-15 — Exact recompute and the reference evaluator.** (S3, A-REF, A-RANGE.) The interval analysis's soundness (the exact-result
rule) and the determinism of `eval_node` across platforms; the media-pipeline edge's interval check.
