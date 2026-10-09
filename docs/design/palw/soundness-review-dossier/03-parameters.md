# 3. Parameters

Values at the base commit `b676927de` unless a branch is named. **INTERIM** = a value written for a never-armed fence or a drill,
explicitly not a production value. **DESIGN** = a document value with no code. **CONSTANT** = a protocol constant whose change is a
new descriptor/version. **TEST-ONLY** = a value only tests and tools use. No value in this file is approved for production: the
approval registry is empty (`misaka-palw-challenge/src/policy.rs:193–221`, `approved_v1`), and every fence named here is refused at
every real height.

## 3.1 The user's targets (2026-10-08 rulings, `activation-readiness-matrix.md` §3a)

| Target | Source |
|---|---|
| An **effective 128-bit false-accept bound** for production, after retries, grinding, multiple relations and adaptive attacks | user ruling §3a "Challenge policy / beacon" |
| `ε_check ≤ 2^-128` per committed claim under the stated challenge/binding assumptions (a target to demonstrate, not a property) | RFC-0011 §15.4 |
| The interim **2 bits** is drill-only; the approval registry stays empty until an external review and bootstrap/grinding attack tests | user ruling §3a |
| OPV assumes at least one capable honest verifier within the deadline | RFC-0015 §2, user ruling §3a |
| OPV window `T_challenge ≥ T_beacon + T_fetch + T_check + T_localize + T_file + T_margin`, all measured; collateral from maximum gain, concurrent exposure, detection probability and the collectable slash (detection probability 0 ⇒ no finite collateral) | user ruling §3a "OPV admission" |

## 3.2 Fields, checkers and the kernel's soundness policy

| Parameter | Value | Source | Status |
|---|---|---|---|
| K2-TIR-v1 dense field | `2^127 − 1` | `misaka-palw-kernel/src/field.rs:15` | CONSTANT |
| bits per repetition, `2^127 − 1` | 126 (`⌊log2 q⌋`) | `field.rs:18` | CONSTANT |
| K2-TIR-v2 moduli | `2^127 − 1, 2^107 − 1, 2^89 − 1` (all Mersenne primes; product ≈ 2^323) | `field.rs:21` | CONSTANT |
| bits per repetition, K2-TIR-v2 (descriptor-wide, weakest modulus) | 88 | `family.rs:107–113`, `descriptor.rs:106–113` | CONSTANT |
| repetitions `t` (v1, v2, v3) | 2 | `descriptor.rs:154` | CONSTANT of the descriptor (a plan may not lower it, `check.rs:195`) |
| whole-claim target | 128 bits | `descriptor.rs:154` | CONSTANT |
| binding term | 256 bits (BLAKE2b-512 collision, classical) | `descriptor.rs:154` | CONSTANT (A-H2: quantum figure open) |
| derived whole-claim bits | `min(t·b − ⌈log2 R⌉, binding) − 1` | `plan.rs:309–317` | CONSTANT (formula) |
| descriptor ceilings | relations `2^16`; plan `2^24` B; positions `2^21`; claim verifier work `2^64`; claim evidence `2^50` B; court bytes `2^32`; court work `2^36` | `descriptor.rs:156–162` | CONSTANT |
| prosecution policy | court deadline 20 DAA; sessions/claim `2^10`; public bytes `2^40`; verifier RAM `2^36`; retained state `2^32` | `consensus/core/src/palw_kernel_route_v1.rs:174–180` | INTERIM |

## 3.3 The challenge contract and the onboarding policy

| Parameter | Value | Source | Status |
|---|---|---|---|
| hash suite | keyed BLAKE2b-512, key = domain, payload = `u64_le(len) ‖ borsh` | `misaka-palw-challenge/src/hash.rs:10–24` | CONSTANT (proposed ids) |
| field sampler | `GF(2^127 − 1)`, 127-bit mask, reject `p` | `seed.rs:160` | CONSTANT |
| stream bound | `2^24` words per labelled stream | `seed.rs:74` | CONSTANT |
| `reference_policy_v1` security bits / retries / soundness id | 40 / 2 / `soundness/unreviewed-test-only/v1` | `policy.rs:242–245` | TEST-ONLY |
| onboarding `k` | 2 | `consensus/core/src/palw_conformance_evidence_v1.rs:604` | INTERIM |
| onboarding anchor delay | 2 DAA | `:605` | INTERIM |
| onboarding beacon window | 120 DAA | `:606` | INTERIM |
| onboarding settlement depth `D` | 2 DAA | `:607` | INTERIM |
| onboarding repetitions | 1 | `:608` | INTERIM |
| onboarding security bits | **2** | `:609` | INTERIM (drill-only) |
| onboarding retries | 2 (three counted attempts) | via `reference_policy_v1`, `policy.rs:245` | INTERIM |
| evidence deadline after lock / refutation window | 60 / 80 DAA | `:631`, `:634` | INTERIM |
| chain bound on a scope | 4,096 checks; `2^20` prompt-token draws; `2^12` decoded tokens | `:638–640` | INTERIM |
| scope v1 default fault model | vectors 500,000 ppm (`f = 1/2`); leaves 62,500 ppm (`f = 1/16`) | `:117–118` | DESIGN default; **no reviewed policy approves it** |
| approved fault model | `(1,000,000, 1,000,000)` ppm — only for the unreviewed test soundness id | `:77–79` | TEST-ONLY (vacuous: `f = 1`) |
| scope bits | `⌊n · ppm · 14,426 / 10^10⌋`, min over families | `:174–189` | CONSTANT (formula) |

## 3.4 OPV terms (`palw_panel_free_v1`, never armed)

| Parameter | Value | Source | Status |
|---|---|---|---|
| base window + verification horizon | 40 + 10 = 50 DAA | `consensus/core/src/palw_panel_free_v1.rs:59` | INTERIM |
| budgets cold / check / localize / disclose / court / carrier / reorg | 10 / 10 / 2 / 8 / 3 / 2 / 2 DAA | `:61–67` | INTERIM (unmeasured; MEAS lane) |
| reservation per claim | 1,000 BILI | `:70` | INTERIM |
| work credit / external gain bound | 5 / 10 BILI | `:71–72` | INTERIM |
| assumed detection | 500 ‰ | `:73` | INTERIM (an assumption, not a derivation) |
| live claims per producer / total | 3 / 32 | `:74–75` | INTERIM |
| default burn | 100 ‰ | `:76` | INTERIM |
| claim reward / default penalty | 5 / 100 BILI | `palw_kernel_route_v1.rs:168`, `:166` | INTERIM |
| accuser reward | 500 ‰ | `:165` | INTERIM |
| court deadline / proof grace / liability | 20 / 10 / 200 DAA | `:160–162` | INTERIM |
| dismissed proof fee | 0.1 BILI | `:164` | INTERIM |
| adjudications per block / court work per block | 64 / `2^30` | `:169–170` | INTERIM |
| claim seal delay | 1 DAA | `:172` | INTERIM |
| *on `g14/r4-fixes`*: admission fee; fresh-producer slots; seal deposit | 1 BILI; 16; 1 BILI | `g14/r4-fixes:consensus/core/src/palw_panel_free_v1.rs` (economics), commit `00b2491cc` (seal deposit) | INTERIM, not in the base |

Derived at the interim terms: maximum gain `5 + 5 + 10 = 20` BILI; required reservation 120 BILI at the base commit, 240 BILI with
self-recoup priced (`g14/r4-fixes`); hard deadline `a + 50 + 20 + 10 = a + 80`; verifier start cutoff `a + 50 − 24 = a + 26`.

## 3.5 Designs that carry numbers

| Parameter | Value | Source | Status |
|---|---|---|---|
| v4 leaf tile `T` | 4,096 elements | `k2/real-scale:docs/design/palw/k2-real-scale.md` §1.1 | DESIGN |
| v4 segment `S` | 1,024 positions | ibid. §1.2 | DESIGN |
| v4 response part / open sessions per demander | 1 MiB / 4 | ibid. §4 | DESIGN |
| 9B-8k: positions, instances/position, committed values/position, evidence/position | 8,192; 2,210; 13,323; 1,536,521,593 B | `coverage-p1p2-record.md` §1.2 | measured (probe) |
| 9B-8k per-prosecution public bytes (v4) | ≈ 7.5 GB; producer DA obligation ≈ 16 TB | K2S §2, §7 | DESIGN estimate |
| OPV-BOOT live-claim cap `C`, `k`, `P(C,k)` | 32, 2, 992 | `opv/bootstrap-beacon:docs/design/palw/opv-beacon-bootstrap.md` §6 | DESIGN |
| OPV-BOOT `min_effective_bits` | 128 | ibid. §5.2 | DESIGN |
| complete-check domain | stateless; `N ≤ 1,024` inputs; `≤ 4,096` leaves and `≤ 512 KiB` artifact; `N × work ≤ 2^26`; default at commit + 60 DAA | ibid. §4 | DESIGN |

## 3.6 Formulas the parameters must satisfy

Let `R` be the probabilistic instances of the statement, `t_f`, `b_f` the repetitions and per-repetition bits of family `f`, `R_a`
the retry limit, `g = ⌈log2 G⌉` the grinding bits per beacon, `β` beacons per attempt, `Q` adaptive statements, `B` the binding bits.

| Id | Requirement (for an effective 128-bit bound) | Where it bites |
|---|---|---|
| P-ALG | `min_f(t_f·b_f) − ⌈log2 Σ R_f⌉ − ⌈log2(R_a + 1)⌉ − β·g − ⌈log2 Q⌉ ≥ 129` (the `+1` is the union with the binding term) | per-claim public-coin checks; `t` and `b` |
| P-BIND | `B − 1 ≥ 128` (classical `B = 256`; the quantum figure is a reviewer question) | every commitment |
| P-CONF | per family `⌊n·ppm·14,426/10^10⌋ ≥ 129 + ⌈log2 m⌉ + ⌈log2(R_a+1)⌉ + β·g + ⌈log2 Q⌉`, i.e. `n ≥ ⌈(that)·10^10 / (ppm·14,426)⌉`; and `Σ n ≤ 4,096` (chain bound) | onboarding conformance |
| P-SEL | complete coverage of every claim by some honest verifier; otherwise the bound is `⌊log2(N/(N−s))⌋` bits and no `t` repairs it | A-HV; OPV |
| P-TIME | `B_cold + B_check + B_carrier + B_reorg ≤ W`; `B_disclose + B_carrier ≤ court_deadline`; `B_localize + B_court + B_carrier + B_reorg ≤ proof_grace`; `Σ B ≤ W + court_deadline + proof_grace`; `liability > W + court_deadline + proof_grace` — with *measured* `B_*` (`opv.rs:185`) | OPV window |
| P-ECON | `reservation ≥ ⌈max(gain + default_penalty, ⌈gain / P_dc⌉) / (1 − a)⌉` with `P_dc` derived from the coverage actually assumed; `censorship_cost > gain` | OPV collateral |
| P-BEACON | `G` bounded by an on-chain-enforced quantity (v3: the adversary's withholdable seals and reachable branches), each choice priced; `ε_src ≤ 2^-129` or entered in the composition | every beacon consumer |

Worked instances of every row are in [05-composition.md](05-composition.md) §5.5 and pinned by
`misaka-palw-challenge/tests/composition.rs`.
