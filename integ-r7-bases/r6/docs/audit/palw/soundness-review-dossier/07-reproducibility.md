# 7. Reproducibility

Everything below runs offline from the repository at the dossier's base (`sound/review-dossier`, based on `b676927de`), with the
pinned toolchain (`rust-toolchain.toml`). On the project's shared build host every cargo invocation goes through the build gate
(`~/Downloads/MISAKA-wt-b/buildslot.sh <cargo …>`, which waits for a free slot and ≥ 18 GB of disk and sets `CARGO_INCREMENTAL=0`);
an outside reviewer runs the same commands without it.

## 7.1 The executable checks of this dossier

```text
cargo test --offline -p misaka-palw-challenge --test composition     # §5.5 rows A–M pinned; toy-field Freivalds and CRT experiments
cargo test --offline -p misaka-palw-challenge --lib composition      # the calculator's own edge cases (ceil log2, selection floor, refusals)
```

| Test | Demonstrates |
|---|---|
| `rows_a_b_reproduce_the_kernels_derived_bounds_for_9b_8k` | the calculator equals the kernel's `derived_error_bits` on the 9B-8k inputs (`2^-226` v1, `2^-150` v2) |
| `rows_c_to_f_charge_retries_grinding_and_statements` | the loss terms; `t = 3` restores the margin against `2^40` offline grinding; private salt needs no beacon |
| `row_g_a_sampled_check_is_bounded_by_its_selection_term` | 8 of 8,192 positions: no security from the check |
| `row_h_the_single_false_instance_alternative_drops_the_instance_union` | the Q-04 alternative (175 bits) |
| `rows_i_j_sampled_conformance_interim_and_a_128_bit_scope` | the interim scope gives −11 bits; 211 vectors + 1,686 leaves give 128 under scope v1's fault model |
| `rows_k_l_m_reservation_sealed_beacon_and_interactive_rounds` | the deterrent reservation (240 / 40,960 BILI), a sealed-source beacon (130 bits), `G^β` for 30 rounds (302 bits lost) |
| `toy_field_freivalds_misses_a_fixed_error_with_probability_p_to_the_minus_rank` | over GF(31) with the crate's own sampler: rank-1 error passes ≈ 1/31, rank-2 ≈ 1/961, zero error always (completeness) |
| `toy_crt_an_integer_error_below_the_moduli_product_survives_and_one_at_the_product_does_not` | moduli 7 and 31: every `0 < |e| < 217` survives one modulus; `e = 31` passes with ≈ 1/7; `e = 217` always passes |

## 7.2 The challenge contract and its golden vectors

```text
cargo test --offline -p misaka-palw-challenge --test contract
```

`tests/contract.rs` exercises every rule by the attack it exists for (policy refusals, source eligibility, canonical order and dedup,
lock depth, `BEACON_UNAVAILABLE` without fallback, reorg recompute, forged presented beacons, per-kind seeds, samplers, staged and
Fiat–Shamir rounds, the onboarding lifecycle, conformance evidence). Golden vectors (`contract.rs:349–352`) pin the policy id
(`927ed75be91eb743…`), a beacon output (`ac77cc132c9a804e…`), a challenge seed (`7235de02dfc706e7…`) and the first Freivalds word
(`1131241806272782869`) for the fixture context: any change to a domain, an encoding or a derivation changes them.

The C4 adversarial suite for the contract (sampler chi-square tests, all 720 source permutations, every single-field edit of a
presented beacon, transcript substitution) is on branch `adv/c4-e2e` (`cargo test --offline -p misaka-palw-challenge --test
adv_c4_contract`).

## 7.3 The kernel (S1–S3, S7)

```text
cargo test --offline -p misaka-palw-kernel
```

| Test | Statement |
|---|---|
| `src/field.rs` unit tests: `multiplication_agrees_with_double_and_add_for_every_modulus`, `integers_map_with_sign_and_round_trip_small_values`, `an_integer_error_below_the_moduli_product_survives_in_some_modulus` | field arithmetic for `e ∈ {127, 107, 89, 61}`; `i128` mapping; S2's CRT lemma |
| `src/challenge.rs` `the_seed_moves_with_every_commitment_and_labels_separate_streams` | kernel seed binding and label separation |
| `tests/k2_e2e.rs`: `an_honest_claim_passes_with_its_derived_bound`, `a_false_matmul_scalar_is_found_by_freivalds_localized_and_convicted_by_the_public_court`, `every_single_scalar_lie_in_one_product_is_caught`, `a_false_value_in_every_exact_family_is_recomputed_and_convicted`, `a_challenge_drawn_before_the_evidence_was_bound_is_refused`, `a_plan_cannot_omit_weaken_overclaim_or_underprice`, `weight_products_are_batched_across_the_scope_so_checks_and_weight_reads_do_not_grow_per_token` | S1, S3, L-COV, batching |
| `tests/k2_wide.rs` (7 tests) | S2: fewest moduli, `i128` products, a lie aliasing to 0 mod `2^127 − 1` caught by `2^107 − 1` |
| `tests/k2_family_review.rs`: `a_lie_at_every_node_of_the_reference_classes_is_localized_and_convicted`, `every_declared_family_has_a_checker_a_public_court_and_a_finite_bound` | coverage and court completeness at reference scale (v1–v3 courts) |
| `tests/k2_adversarial.rs` | permuted history, swapped expert, uncommitted weight (DA path), an unsound custom suite refused, stale-anchor receipts after a reorg, identical verdicts across runs and scopes |
| `tests/k2_public.rs`, `tests/k2_pipeline.rs`, `tests/k2_ledger*.rs` | public court from bytes only; pipeline edges (v3); ledger lifecycle |
| `tests/k2_opv.rs` (21 tests), `tests/k2_opv_pipeline.rs` | S7: window rule, reservation, caps, censorship-cost admission, outsider conviction before and after Final, default path, replay/reorg |

Part II's private-sketch baseline (not the public-challenge route; listed for completeness): `cargo test --offline -p
misaka-palw-tir-sketch --test soundness` (a tamperer who knows the vector passes, one who does not is caught; the modulus ladder; a
toy prime `p = 101` error-rate test).

## 7.4 Consensus: conformance, OPV fence, and lane D's real-node `g14_*` tests

```text
cargo test --offline -p kaspa-consensus-core --lib palw_conformance_evidence     # the interim onboarding policy's numbers
cargo test --offline -p kaspa-consensus-core --test rfc0015_panel_free             # the OPV fence is dormant, hashed Some-only, refused when armed
cargo test --offline -p kaspa-consensus --lib g14_kernel_route                     # the K2 route through the real node path
cargo test --offline -p kaspa-consensus --lib g14_opv                              # OPV on the real node path
cargo test --offline -p kaspa-consensus --lib g14_conformance                      # tag-109 conformance evidence on chain
cargo test --offline -p kaspa-consensus --lib g14_onboarding                       # binding, attestation, refutation, envelope
cargo test --offline -p kaspa-consensus --lib g14_c4r3                             # C4 round 3 PoCs and controls
cargo test --offline -p kaspa-consensus --lib g14_c4r3 -- --ignored                # the round-3 defects still open in this base
```

Selected cases (all in `consensus/src/pipeline/virtual_processor/tests/`): `g14_kernel_route_a_covered_lie_is_convicted_by_an_
outsider_through_the_real_path`, `…_a_withheld_position_is_a_demand_then_a_default_never_a_conviction`, `…_final_waits_the_proof_grace_
so_a_late_served_lie_is_convicted_before_final`, `…_spam_demands_cannot_hold_final_past_window_end_plus_court_deadline_plus_proof_grace`,
`…_replay_and_reorg_reach_the_same_roots`; `g14_opv_a_lying_claim_is_convicted_by_a_fresh_outsider_before_final`,
`g14_opv_a_lie_that_finalized_is_convicted_within_liability_and_its_fact_is_withdrawn`,
`g14_opv_an_honest_claim_finalizes_with_no_panel_and_exports_a_panel_independent_beacon_fact`;
`g14_conformance_evidence_passes_only_after_an_unrefuted_window_and_the_class_activates`,
`g14_conformance_forged_evidence_is_refuted_withheld_evidence_defaults_and_attempts_are_exhausted`. Each outsider in these tests is a
fresh verifier built from the node's read API with its **own salt** (`g14_kernel_route_e2e.rs:454–471`).

**In this base the `g14_c4r3` ignored tests for F-C4R3-02, -03 and -05 fail by design** (they assert the safe property of an open
defect). Their fixes, with the PoCs un-ignored, are on branch `g14/r4-fixes` (merge `d9f086c9a`); run the same commands there, plus
`cargo test --offline -p misaka-palw-kernel --test k2_ledger --test k2_opv --test k2_ledger_route` for the kernel regressions listed in
`adversarial-e2e-record.md` ("G14-R4 fixes").

## 7.5 Branches the dossier cites

| Ref | What | Kind |
|---|---|---|
| `b676927de` | the dossier's base (integration HEAD when the dossier started) | code + docs |
| `k2/real-scale` `b8870bde0` | `docs/design/palw/k2-real-scale.md` — v4 element courts (S4) | design only |
| `opv/bootstrap-beacon` `f2dca0e6d` | `docs/design/palw/opv-beacon-bootstrap.md` — dependency graph, complete-check bootstrap, grinding table, effective-bits formula (S8) | design only |
| `g14/r4-fixes` `d9f086c9a` | F-C4R3-02/03/05, GAP-R7, GAP-5 fixes; bonded claim seals (`00b2491cc`) for the sealed-source beacon v3 | code |
| `adv/c4-e2e` | `adv_c4_contract` and the C4 round-1 suites | tests |
