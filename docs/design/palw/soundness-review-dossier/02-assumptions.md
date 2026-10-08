# 2. Assumptions

Each assumption is named once and referenced by the statements ([01](01-statements.md)) and the composition ([05](05-composition.md)).
"Enforced" means code makes the assumption's premise true by construction; "operating" means the network must make it true and no
code can; "review" means the reviewer is asked to accept, reject or quantify it.

## 2.1 Hash, random-oracle and quantum assumptions

| Id | Assumption | Used by | Kind |
|---|---|---|---|
| A-H1 | **Random oracle.** Keyed BLAKE2b-512, with each distinct domain string (≤ 64 bytes) as the key, behaves as an independent random oracle per domain. Keys of different lengths are distinct BLAKE2b parameter blocks (key length is in the parameter block), so a domain that is a prefix of another (`…/WORK-BEACON/V1` vs `…/WORK-BEACON/ITEM/V1`) is still a separate oracle. Used for seeds, labelled streams, beacon accumulators and the challenge anchor. | S1, S2, S5, S6 | review |
| A-H2 | **Binding.** BLAKE2b-512 collision and second-preimage resistance, for tensor commitments (row and column Merkle trees), position / segment / claim roots, param commitments, the artifact root and object ids. The kernel prices the binding term at `binding_bits = 256` (`descriptor.rs:154`), the classical collision bound for a 512-bit hash. Against a quantum adversary the generic collision bound (Brassard–Høyer–Tapp) is ≈ `2^(512/3) ≈ 2^170`; second preimage ≈ `2^256` (Grover). The reviewer is asked which figure the production policy uses. | S1–S5, L-COV | review |
| A-H3 | **Injective encodings.** Every hashed object is a canonical Borsh encoding of a fixed type (length-prefixed vectors, tagged enums, `RootV1::Absent` as typed absence — never a zero wildcard); `h()` prefixes the payload length; the kernel's seed and stream concatenate fixed-width fields only (`misaka-palw-kernel/src/challenge.rs:33–87`). Two different statements therefore never share a seed preimage. | S6 | enforced (review the encodings) |
| A-H4 | **QROM** for the transcript-bound Fiat–Shamir mode, and for any seed an adversary can influence (quantum search gives a quadratic advantage in finding a bad seed among `G`). No checker uses an interactive mode today. | S6.6, S8 | review (external gate, RFC-0007 §VI.6) |

## 2.2 Semantics assumptions

| Id | Assumption | Kind |
|---|---|---|
| A-REF | The reference evaluator (`misaka-palw-tir` interpreter; `eval_node`, `eval_primitive`) *is* the semantics of a class. Exact checks and courts compare with it; conformance (S5) compares implementations with it. | definition |
| A-RANGE | The admission interval analysis (`misaka_palw_tir::interval::analyze_ranges`, `check.rs:111–115`) is sound: no partial sum of an exact primitive of an honest execution overflows its type. Needed for completeness of S1–S3 and for the span bound in S2. | review (RFC-0002 spec 04b §7) |
| A-COV | The plan checker's coverage (`check.rs:126–214`) is the program's whole dependency graph (L-COV preconditions i–iv). | enforced by code; review the code |
| A-FAULT | **Conformance fault model.** A faulty implementation set disagrees with the reference on at least `f` of a family's uniform draw space (`vector_fault_ppm`, `leaf_fault_ppm`). Approved per soundness policy (`approved_fault_model_v1`); today only the unreviewed test id, with `f = 1`. | review |

## 2.3 Honest-verifier liveness (the OPV premise)

**A-HV.** For every claim `c` of an `OptimisticPublicVerification` class admitted at DAA `a`, there exists **at least one capable
honest verifier** `V` such that:

1. `V` starts no later than `verifier_start_cutoff(a)` (`opv.rs:166`) and fetches the claim's public material within `B_cold`;
2. `V` checks a scope `S_V(c)` of the claim within `B_check`, with **private coins**: its vectors are derived from a salt drawn from a
   CSPRNG after `c`'s commitment is accepted and never revealed (`OutsiderV1::salt`, `misaka-palw-kernel/src/ledger.rs:1935–1944`;
   `FreshVerifierV1::check_salted`, `public.rs:158–166`);
3. `V` files a demand or a proof inside the window, and follows a served demand with a proof inside the proof grace.

The **coverage** `Cov(c) = ⋃_V S_V(c)` over the honest verifiers determines the selection term of §5.4: with complete coverage
there is no selection term; with `V` checking `m` of `P` positions uniformly, a one-position lie escapes with probability
`1 − m/P`. RFC-0015 §2 states A-HV as an operating assumption; no code establishes it, and "anyone can check" is not "someone
checks". The interim policy's `assumed_detection_permille = 500` (`palw_panel_free_v1.rs:73`) is a number standing for A-HV, not a
derivation from it.

**Scope of A-HV under K2S v4.** A whole-claim check of 9B-8k reads ≈ 12.6 TB (K2-TIR-v2 measurement, `coverage-p1p2-record.md`
§1.2) or ≈ 16 TB of position material (v4 estimate, K2S §2); the interim budgets are `B_cold = B_check = 10 DAA` with a DAA of at
least 120 s (`rfc-0012-policy-proposal.md:14`). The reviewer is asked whether A-HV with complete coverage is a tenable assumption
at that scale (Q-01).

## 2.4 Beacon source assumptions

| Id | Assumption | Status |
|---|---|---|
| A-B1 | **Source independence / acyclicity.** A source's validity is independent of any challenge consuming it; the candidate and work depending on it are excluded; a Panel-licensed Final never seeds a Panel assignment. | enforced (`eligibility_v1`, `beacon.rs:145–179`); the dependency graph's acyclicity is OPV-BOOT's design (§3 of its doc) |
| A-B2 | **An honest unpredictable source.** With probability at least `1 − ε_src`, at least one *selected* source's item `H(ITEM; profile, work id, execution commitment)` is unpredictable to the adversary until after the adversary's last choice that can change the selected list or its items. | review — **violated** for the last contributor of the v2 accumulator (SG-01); for v3 it depends on the selection rule (Q-02) |
| A-B3 | **Bounded steering.** The adversary can make at most `G` distinct beacon outputs canonical per beacon (output selection, withholding, timing, fork choice, concentration), each choice costing something recorded on chain. | review — v2: `G` = the adversary's offline hash budget; v3 design: `G ≤ 2^a · F` |
| A-B4 | **Settlement.** No canonical reorg deeper than `settlement_depth_d` past the `k`-th source's settlement after the lock, except as a counted retry (`reorg/branch-relative-recompute-and-roll-back-dependents/v1`). This rests on PALW fork-choice safety; see the **fork-choice safety analysis (internal, FINX)**, a release blocker, not described here. | operating |
| A-B5 | **Bootstrap.** Sources exist: eligible classes have Finals inside the window. With derived eligibility the least fixed point is empty unless a beacon-free (complete-check) path exists (SG-02). | design |
| A-B6 | **Computation cost of an alternative sample.** RFC-0007 §VI.3 requires that "an alternative valid entropy sample requires additional paid useful computation". For a complete-check bootstrap class (≤ 2^26 work units per complete check, inputs ≤ 1,024) and for any class whose job the adversary may post, a new sample costs a job fee and a seal deposit, not meaningful computation (Q-03). | review |

## 2.5 Network synchrony

| Id | Assumption | Parameter |
|---|---|---|
| A-NET-1 | An object broadcast at DAA `t` that fits its carrier is included in the canonical chain by `t + B_carrier` (`carrier_daa`), against the adversary's censorship capacity priced by `censorship_cost` (`opv.rs:177`). | interim `carrier_daa = 2` |
| A-NET-2 | Reorgs of an included filing are bounded by `B_reorg` (`reorg_slack_daa`). | interim 2 |
| A-NET-3 | DAA advances at a bounded rate: at least 120 s per DAA (a DAA step needs one heartbeat slot, `rfc-0012-policy-proposal.md:14`; operational notes ≈ 150 s). All budgets are in DAA; seconds are never silently equated with DAA. | derived |
| A-BUDGET | The fresh-verifier budgets `B_cold, B_check, B_localize, B_disclose, B_court` hold for the class on the hardware the network assumes. | interim values; MEAS lane measures them |
| A-DA | Public material is retrievable within `B_cold` by any verifier, or a demand obliges the producer to serve it on chain (default otherwise). | enforced (demand/default path); availability of the off-chain stream is operating |

## 2.6 Economic rationality

| Id | Assumption |
|---|---|
| A-ECON-1 | Producers maximise expected profit; a lie is deterred when `reservation · (1 − a) · P_dc ≥ gain` (S7.2), `P_dc` the probability of detection **and** enforceable conviction. |
| A-ECON-2 | The gain of one claim is at most `claim_reward + work_credit_per_claim + external_gain_bound`; the external bound (fork-choice influence, external settlement) is stated, not derived. |
| A-ECON-3 | Concurrent lies do not share collateral (enforced: free-collateral reservation per claim, live caps); correlated detection failure (no watcher at all) is an A-HV failure, not an economic one. |
| A-ECON-4 | Colluders recoup at most the accuser share `a` of their own slash (enforced by the accuser seal on `g14/r4-fixes`; priced by `÷(1 − a)` there; not priced at the base commit — O-C4R3-R7). |
| A-ECON-5 | A default (withheld material) costs `default_penalty` and yields no Final, so no reward; pre-Final external gains are inside A-ECON-2. |

## 2.7 Chain assumptions

| Id | Assumption |
|---|---|
| A-CHAIN | The chain makes progress and its PALW fork choice is safe enough that a beacon locked at depth `D` and a Final at its settlement are not reverted except by counted, branch-relative recomputation. **Reference: fork-choice safety analysis (internal, FINX).** The dossier makes no claim about fork-choice mechanisms. |
