# Beacon conformance of a runtime pack — implementation record (lane C)

Branch `onboard/c-beacon-pack`, 2026-10-08. Tooling only: nothing here is consensus, adds a fence, a wire id or an activation, and
nothing here changes `misaka-palw-challenge` (the one contract; it is used as a dependency and the Lead's `47bacfa74` is cherry-picked).

```text
STATIC SEMANTIC ADMISSION → COMMIT FIRST → FUTURE PALW WORK BEACON → INDEPENDENT PROBABILISTIC CHECK → mismatch → EXACT PUBLIC COURT
```

**What a PASS here is, and is not.** A pass means: every check the committed scope derives was drawn from the seed this process
recomputed from the facts it was given, ran on every required implementation, and agreed; and a fresh derivation reproduced the
evidence byte for byte. It is a **sampled** check with a **conditional**, scope-derived error bound. It is not full-scope fidelity,
not semantic admission, not G14, not a claim that the beacon is unbiased, and it is not a statement about any chain while the facts
are `Synthetic` (they are, everywhere below: the node RPC that serves canonical beacon facts does not exist yet). The challenge
policy is `reference_policy_v1` with caller numbers and is **UNAPPROVED**; no approved (checker suite, challenge policy, soundness
policy) tuple exists.

## 1. What exists

| piece | where | note |
|---|---|---|
| bind a pack into `ConformanceCommitmentV1` | `runtime_pack/commit.rs`, `palw-class pack commit-conformance` | every root re-derived from the artifact; commitment bytes = the contract's `statement_root()` |
| loader boundary for canonical beacon facts | `runtime_pack/facts.rs` (`BeaconFactSource`, `ChainBeaconFactsV1`) | lane D implements it over RPC; `FileFactSource` / `MemoryFactSource` exist today |
| selection, evidence, resume, verify | `runtime_pack/beacon_run.rs`, `pack run-conformance`, `pack verify-conformance` | contract functions only: `collect_work_beacon_v1`, `verify_work_beacon_v1`, `challenge_seed_v1`, `ChallengeStreamV1`, `verify_conformance_evidence_v1` |
| per-implementation vector runner | `runtime_pack/conformance.rs` (`TripleRunner`) | digests from each implementation's own outputs |
| implementation revisions | `misaka-palw-sdk/build.rs` | BLAKE2b-512 of the source of reference / ref2 / typed backend / container reader / checker, taken at build |
| tests | `misaka-palw-sdk/tests/runtime_pack_beacon.rs` | 17 tests, all attack-by-attack below |

Commits (cherry-pickable, in order): `8906aa73b` bind commitment · `43fcb810d` facts boundary · `d7d50b1d5` triple runner ·
`62d8c5205` challenge/evidence/verify · `97020c2ae` CLI · `3a4218e55` tests · `55326aa07` Final path (contract `47bacfa74`) ·
`c6d45d6ad` reorg-undone revalidation.

### Commands (`palw-class pack …`)

```text
commit-conformance --pack <dir> --artifact <declared class file> --state <dir> --network testnet-12 \
    --chain-genesis <hex128|label:x> --ruleset-id <hex128|label:x> [--class-id <prefix>] [--candidate-id <hex128>]
    [--k --delay --window --depth --repetitions --security-bits --retry-limit]            # the UNAPPROVED test policy
    [--vectors --prompt-len --decode --leaves --vector-fault-ppm --leaf-fault-ppm]        # the committed scope + fault model
run-conformance    --pack --artifact --state --commitment <prefix> --facts <facts.json> [--no-ref2] [--no-exec] [--max-checks N]
verify-conformance --pack --artifact --state --commitment <prefix> --facts <facts.json> --evidence <evidence.borsh> [--no-rerun]
synthetic-facts    --state --commitment <prefix> --out <facts.json> [--position N] [--works N] [--tip N]   # SYNTHETIC, labelled
conformance-status --state <dir>                                                                          # the append-only ledger
```

Exit codes — run: `0` evidence written and the contract accepts it, `2` evidence written but not a pass, `3` pending
(WaitingRandomness / BEACON_UNAVAILABLE / interrupted: rerun resumes), `1` refused. Verify: `0` PASS, `2` FAIL or not a pass,
`3` pending, or results not re-executed (`--no-rerun` can never be a pass).

### Files

```text
<state>/ledger.json                       append-only: COMMITTED, WAITING_RANDOMNESS, BEACON_UNAVAILABLE, CHALLENGE_RESOLVED,
                                          INTERRUPTED, EVIDENCE, INVALIDATED, REVALIDATED  (atomic rewrite)
<state>/<stmt32>/commitment.borsh         borsh(ConformanceCommitmentV1); the dir name is its statement_root prefix
                 policy.borsh params.borsh   the policy it names; the parameters it is re-derived from
                 summary.json               human record (recomputed, never trusted)
<state>/<stmt32>/seed-<seed32>/           one directory per challenge seed (a reorg is a new directory)
                 facts.json facts.borsh     the exact public facts used      beacon.borsh   the presented WorkBeaconV1
                 selection.borsh            multiproof.borsh   the authenticated openings
                 checks/<id>.json           atomic completion records (binding = seed, statement, implementation set, selection, executors)
                 evidence.borsh evidence.json run.json   (run.json holds the machine-dependent measurements; never the evidence)
                 INVALIDATED                advisory mark; verification never reads it
```

Facts file `misaka.palw.beacon-facts.v1` (JSON, digests 128 lowercase hex): `provenance{kind: synthetic|node, label}`, `policy_id`,
`commitment_position`, `challenge_epoch`, `eligible_profiles[]`, `excluded_profiles[]`, `tip_position`, `events[]` with
`kind, source_profile_id, canonical_work_id, execution_commitment, accepted_position, settlement_position, occurrence_index,
claim_final, da_satisfied, validity_independent, depends_on_profiles[], final_path{PANEL_LICENSED{panel_seed_id,panel_epoch}|PANEL_INDEPENDENT}`.
Chain, ruleset, subject kind, statement root and the policy come from the commitment, never from the facts.

## 2. The committed scope and its error bound

The scope (`test_scope_root`, fixed before any randomness) is: per repetition (the policy's `repetition_count`), `V` prompts of
`1..=P` tokens plus `D` greedily decoded tokens, run on the reference evaluator, the independent second implementation and the typed
backend (logits and every commit point at every position); and `L` artifact leaves (the inventory's own ≤32 KiB leaves, drawn
uniformly over the leaf index space) opened against the artifact root and decoded by each implementation's own tensor decoder.

`derived_epsilon_bits` is an integer lower bound on `−log2 ε` for a stated fault model: a fault visible on at least `f` of a family's
draws, independent uniform draws, `ε ≤ (1−f)^n ≤ e^(−fn)`, `−log2 ε ≥ f·n·log2 e` (constant 1.4426, rounded down), the smaller of the two
families. It is conditional on that model and speaks of no fault outside the two families. A scope that derives fewer bits than the
policy's `security_bits` is refused at commit time, before the beacon exists (`SCOPE_CANNOT_MEET_POLICY`).

## 3. Cases

Tests: `CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_LTO=off cargo test --offline --release -p misaka-palw-sdk --test runtime_pack_beacon`
(release because the pack fixtures and the real checkpoint share one build; a tiny Llama fixture pack with a class declared on
testnet-12). Result on `c6d45d6ad`: **17 passed, 0 failed, 5.6 s**; the existing `--test runtime_pack` suite: **9 passed**.

| # | case | test | result | status |
|---|---|---|---|---|
| 1 | commitment binds artifact, program, tokenizer, layout, plan, kernel, policy, calibration, implementation set, scope; same inputs → same commitment | `a_commitment_binds_every_root_from_the_artifact_and_any_change_is_a_new_commitment` | all roots non-zero, `constraint_root` typed `ABSENT`, provenance empty | PASS |
| 2 | any change of policy (k, delay, repetitions), scope (vectors, fault model, required impls), chain, ruleset, candidate, plan positions → a new commitment moving the named field | same | 10 variations, each a distinct statement root | PASS |
| 3 | changed layout = different class = different commitment; ambiguous network refused; a run for the old commitment with the other class file is stale | `a_changed_layout_is_a_different_class…` | `AMBIGUOUS_CLASS`, `layout_root`+`candidate_id` differ, `COMMITMENT_STALE` | PASS |
| 4 | no exact layout → `LAYOUT_REQUIRED` and no state written; scope the policy cannot use → `SCOPE_CANNOT_MEET_POLICY`; oversized/empty scope; invalid policy | `a_pack_without_an_exact_layout…` | all refused before the beacon | PASS |
| 5 | static admission first: a program no reference kernel expresses → `FRONTEND_REQUIRED`; an expressible one → plan root, hypothetically armed, shipped schedule `KERNEL_NOT_ACTIVE` | `static_admission_comes_first…` | | PASS |
| 6 | epsilon is an integer conditional bound | `the_derived_epsilon_is_an_integer…` | 14 vectors/4 reps/50 % → 40 bits; 1 vector → 2 bits | PASS |
| 7 | WaitingRandomness (collecting; candidate not yet at depth D), BEACON_UNAVAILABLE counted, retry limit, no fallback, ledger | `waiting_unavailable_and_retries_are_pending_states…` | no seed directory created while pending; `RETRY_LIMIT_EXHAUSTED` | PASS |
| 8 | beacon sources: heartbeat, BASE-0, EXEC_TX, EXEC slice, receipt-only, provisional, panel receipt; unfinalized; DA-unsatisfied; validity-dependent; committed before S; outside window; duplicate work; candidate self-beacon; work depending on the candidate; non-eligible profile | `beacon_sources_that_are_not_fresh_final…` | none ever locks; missing self-exclusion / self-eligible facts refused | PASS |
| 9 | facts naming another policy → `POLICY_SUBSTITUTED`; a different valid policy in the state → `POLICY_SUBSTITUTED`; `final_path` round-trips, missing path is `FACTS_MALFORMED` | `a_facts_file_that_names_another_policy…` | | PASS |
| 10 | end to end: commit → facts → evidence → fresh verify (re-execution) PASS; `--no-rerun` never a pass; same pack + same history → byte-identical evidence in another state dir, shuffled arrival order, later tip; records reused | `commit_beacon_evidence_verify_passes_reproduces_byte_for_byte…` | 30/30 checks (3 reps × (2 vectors + 8 leaves)); `derived_epsilon_bits` 8 | PASS |
| 11 | interruption after 4 checks, damaged record re-run, resume gives the uninterrupted bytes; enabling a skipped executor re-runs its records | `an_interrupted_run_resumes_from_its_records…` | reused 3, executed 27 | PASS |
| 12 | reorg: new history → new seed, old evidence retained + `INVALIDATED`; old evidence under the new history refused; undone reorg revalidates; reorg removing sources before the lock → pending | `a_reorg_is_a_different_challenge…` | | PASS |
| 13 | presented beacon reordered / duplicated / non-canonical / substituted / dropped / forged accumulator, output, anchor, lock | `a_presented_beacon_that_is_reordered…` | 9 forgeries, all `BEACON_NOT_CANONICAL` | PASS |
| 14 | forged evidence, every field (24 edits incl. seed, output, anchor, lock, each root, counters, epsilon, status) and garbage bytes | `forged_evidence_is_never_a_pass…` | none passes | PASS |
| 15 | SKIPPED (independent impl off) is never a pass; SKIPPED forged to PASSED with counters is `EVIDENCE_NOT_REPRODUCED`; incomplete writes no evidence | `skipped_and_incomplete_are_never_a_pass…` | | PASS |
| 16 | a disagreeing implementation (injected into a leaf check and two vector checks) is a failed check, evidence `Failed`, never a pass | `a_disagreeing_implementation_is_a_failed_check…` | | PASS |
| 17 | artifact byte flipped after commit; verification-plan root edited (in place and renamed); implementation-set root edited → stale/invalid | `a_commitment_whose_artifact_layout_plan…` | `COMMITMENT_STALE` naming the field | PASS |
| 18 | CLI end to end with exit codes (3 pending ×2, 0, 0, 3 `--no-rerun`, 2 forged) | `the_cli_commits_resumes_runs_and_verifies…` | | PASS |

### Not covered by these tests (stated, not hidden)

* Facts here are synthetic or hand-made; **no canonical history of any chain** was used. Lane D's RPC loader is the open item.
* The kernel is judged **hypothetically armed**; the shipped schedule has no Active kernel. No on-chain registration exists for any pack here.
* `constraint_root` is `ABSENT` (typed absence): constraint coverage and G14 are lane B / the Lead's gates, bound by the plan root only.
* Epsilon is the integer conditional bound above, not a reviewed soundness analysis; no policy numbers are approved.
* The opening pass reads and hashes the whole artifact once (a Merkle opening without a stored tree needs every sibling); it is recorded
  as hashed bytes, not as sample bytes. A leaf-hash sidecar would make openings O(selected): not built.
* Vector checks are the expensive family (see §4); a scope large enough for a high-bit bound on a large model is hours of compute.

## 4. Real checkpoint: SmolLM2-1.7B-Instruct (Llama, 24 layers, vocab 49,152)

Local checkpoint `hf-ckpt/HuggingFaceTB/SmolLM2-1.7B-Instruct` (no network). The mamba-370m and Qwen3.5-0.8B checkpoints were not
packed: SmolLM2 already had calibration statistics (`phase-h/stats-512.json`) and a known layout, so it was the one the existing pack
tooling could pack in minutes. Machine: Apple M1 Max, 32 GiB, shared with three other agents (load average 7–20 during the runs; wall
times are therefore upper-ish and noisy).

**Pack** (existing tooling, binary of the base tree): `palw-class pack build --model <ckpt> --out smollm2.palwtir --pack ./pack
--stats-in phase-h/stats-512.json --context 512 --stream --prompts 1 --prefill 1 --decode 1
--declare testnet-12:max-context=512:logits-tile=512` — 210 s, max RSS 4.2 GB. Artifact 1,866,690,944 B (class file), 1,332 tensors,
**1,330,311 inventory leaves**, inventory root `e4f8b50a…`, class `c45d7ef3…` (testnet-12, context 512), math libm-v1.

**Commands and results** (binary `palw-class` built from `c6d45d6ad`, sha256 `d1e9b76a…`; state under the lane scratch; all on
SYNTHETIC facts with `k=3, delay=2, window=40, depth=5, repetitions=3, security_bits=4`, scope 1 prompt of 1..=2 tokens + 1 decoded
and 256 leaves per repetition, vector fault model 1,000,000 ppm, leaf fault model 62,500 ppm):

| step | command (`palw-class pack …`) | result | wall | max RSS |
|---|---|---|---|---|
| commit | `commit-conformance …` | statement root `0321d767…`; K2-TIR-v1 plan `942a3350…` at 512 positions, hypothetically armed, shipped schedule `KERNEL_NOT_ACTIVE`; scope derives −log2 ε ≥ 4 | 5.5 s | 33 MB |
| waiting | `run-conformance` on facts with 2 of 3 works, tip 1010 | `WAITING_RANDOMNESS`, exit 3, nothing run | 5.1 s | 33 MB |
| run | `run-conformance` on the locked facts | **PASSED, 771 of 771 required checks run, 0 failed, 0 missing**, evidence `f1e9e534…`, exit 0 | **577 s** | **4.11 GB** |
| verify, no re-execution | `verify-conformance … --no-rerun` | `NOT A PASS (results not re-executed)`, exit 3 | 6.4 s | 33 MB |
| verify, forged evidence | one bit flipped in `evidence.borsh` | `FAIL EVIDENCE_FORGED … selected_tensor_ranges_root`, exit 2 | 7.4 s | 33 MB |
| verify | `verify-conformance …` (fresh process, re-executes everything) | **PASS**: evidence reproduced exactly, exit 0 | **627 s** | **4.12 GB** |

**What was checked, in bytes and work** (run record; the verifier's re-execution measured the same sizes):

| quantity | value |
|---|---|
| artifact bytes authenticated against the committed root (one streamed pass, all leaves hashed) | 1,866,642,528 B in 3.0 s |
| leaves drawn from the seed / of the inventory | 768 of 1,330,311 (0.058 %) |
| sampled leaf bytes decoded by all three implementations | 1,073,984 B (0.058 % of the artifact) in < 30 ms |
| vector checks | 3 (one per repetition), 7 positions in total (prompts of 1–2 tokens + 1 decoded), reference + independent + typed backend, logits and every commit point compared at every position |
| vector time | 565 s ≈ 81 s per position (the reference and the independent implementation decode every tensor to `i128` per position) |
| completion records | 771 atomic records; evidence 942 B, multiproof 1.58 MB, selection 41 KB |
| derived −log2 ε | **4** (3 vector draws at the declared 100 % fault density; the leaf family derives 69 bits at 6.25 %; the smaller counts) |
| process peak RSS | 4.11 GB, mapped-file pages included; the commit/waiting/verify-only steps stay at 33 MB (the artifact is never held whole; the authentication pass keeps `O(k log n)` hashes) |

What this run is and is not: a sampled differential check of three implementations on 0.058 % of the artifact's bytes and 7 positions
of a 512-position class, against a fault model chosen so that a tiny scope reaches 4 bits. It shows the pipeline — commit before
randomness, a beacon from facts, challenge-selected checks on the real artifact, a fresh verifier reproducing the evidence — works on
a 1.87 GB artifact within a 4.1 GB footprint. It does **not** show that SmolLM2's lowering is faithful (no HF reference check: this
pack was built without `--hf-reference`, so `pack verify` reports the HF fit SKIPPED), does not reach the reference policy's 40 bits
(that needs ≈ 56 vector draws at 50 % fault density — about 140 positions, ≈ 3 hours of vector time at this speed, and again to verify), and
says nothing about any chain.

**GAPs recorded by this run**

* `phase-h/stats-512.json` is in the legacy decimal-float format; the build warns "not bit-exact, not for a runtime pack". The pack
  digests and pins it, but the resulting artifact (`e4f8b50a…`) is not the earlier Phase-H class (`d1d5fad6…`). Calibration should be
  re-measured in the bit-exact format before this pack is used for anything but this pipeline test.
* The checks do not scale to the reference policy's security bits on a 1.7B model with the present reference/ref2 evaluators; a
  tiled, range-based evaluator (RFC-0013 §5) is the only way to make vector draws cheap, and is not built. *(Built since, SMALL lane,
  2026-10-09: the independent implementation's row-tiled evaluation, below. It bounds the independent evaluator's MEMORY; it does not make a
  vector draw faster, and the reference evaluator and the typed backend are unchanged.)*
* The authentication pass hashes the whole artifact (3–7 s here; proportional to artifact size); a stored leaf-hash index is not built.
  *(Built since, SMALL lane: the stored Merkle index, below.)*
* Peak RSS is a process maximum, not the evaluators' working set; it includes the mapped artifact pages the typed backend touches.
* A first run on an earlier build of the same sources (`3a4218e55`) also PASSED (771/771, 793 s, 4.1 GB); it is not the recorded result.


## 5. Contract change proposals (Lead)

1. **Own the evidence roots.** `selected_vectors_root`, `selected_tensor_ranges_root`, the three result roots,
   `authenticated_openings_root`, `public_material_locator_root` and `qualifying_source_evidence_root` have no definition in the
   contract; this tool hashes typed Borsh records under tool-local domains `misaka.palw.runtime-pack.*`. Every consumer (lane D, a
   chain verifier, kernel conformance) needs the same roots, so the contract should define them, with golden vectors.
2. **Own the stream scope ids of model conformance** (`pack-conformance/vector/v1`, `pack-conformance/artifact-leaf/v1` are
   `named_id` labels here), and the leaf-index → `(param, layer, row_start)` mapping rule.
3. **`ConformanceCommitmentV1` has no source-provenance field.** The source file hashes, adapter and quantisation identities are bound
   inside `input_and_state_binding_root` today; a dedicated `source_root` would be clearer. `implementation_set_root` should have a
   contract-defined entry shape (role, crate, source digest) rather than a tool-local one.
4. **`BeaconConformanceEvidenceV1` is narrower than RFC-0013 §9**: no `lock_evidence_root`, `soundness_assumptions_root` or measured
   work/bytes/RAM/time. Measurements are in `run.json`, never in the evidence, so evidence stays reproducible; say so in the contract.
5. **`verify_conformance_evidence_v1` cannot detect forged result roots** (it checks the commitment, policy, beacon, seed, status and
   counters). A consumer must re-derive; the contract could take the selection and per-check digests as inputs, or document that only
   re-execution closes this.
6. **Epsilon for sampled families** is a tool-defined integer bound from a declared fault model. The contract (or the soundness
   policy) should define the formula and the fault-model record so `derived_epsilon_bits` means one thing everywhere.
7. **`BeaconContextV1` carries no candidate id**; self-exclusion is a consumer check (`FACTS_MISSING_SELF_EXCLUSION`). Putting the
   candidate in the context would make `eligibility_v1` refuse it by construction.
8. **`OnboardingRecordV1::apply` has no way to record a commitment that exists without a chain registration**, so this tool keeps its
   own ledger; the retry count is the number of `BEACON_UNAVAILABLE` entries per candidate (allowed: `retry_limit + 1` windows).

---

## 6. RFC-0013 §5 and §7.2 (SMALL lane, 2026-10-09): the row-tiled independent evaluator and the stored Merkle index

Nothing here is consensus, nothing is armed, and no court limit, sizing constant or id moved (the court's maximum bytes and work are exactly what they
were: a tool that reads less memory does not hide what the court would read).

**The row-tiled evaluator (`misaka-palw-tir-ref2::tiled`).** The independent implementation's `MatMul` and `Gather` can consume a param in row tiles
through a `RowSource` instead of holding it whole as `i128` (a 1.9 GB artifact is 16× that). Three forms are tiled: a param that is a `MatMul` operand,
a `Gather` table, and — since this is how the lowerer declares every linear layer (`lower_linear`: weights `[out, in]`, read through `Transpose [1, 0]`) — a
**rank-2 param read through a `Transpose [1, 0]` whose only reader is one `MatMul`**: that transpose node is never built (it is neither committed, carried,
nor the logits), and the tiles of the stored `[out, in]` rows feed the product directly. Any other reader of a large param loads it whole, and is counted
(`TileReport::whole_loads`, `whole_peak_elems`) or, under `TiledParams::strict`, refused by name.

| Claim | How it is held | Test |
|---|---|---|
| bit-identical to the whole-tensor evaluation | `MatMul` sums are order-free (04b §6.3: `P + N`, each total range-checked), so tiles add to the same per-output accumulators in any order and are finished in the output's linear order; every product is the exact 256-bit `Wide::mul_i128`; `Gather` is a copy with the same `Index` refusal at the same element; the shape rules are the whole path's, on the declared shapes | `tiled_evaluation_equals_whole_evaluation_on_random_programs` (250 random programs × tiles 1/2/3/5/10⁶ × 3 positions, values AND refusals), `a_whole_run_through_tiles_equals_the_run_it_replaces`, `a_tiled_matmul_equals_the_whole_matmul_values_and_refusals`, `a_matmul_through_a_fused_transpose_equals_the_matmul_of_the_transposed_tensor`, `weights_read_through_the_lowerers_transpose_are_tiled_never_built_and_the_run_is_the_whole_run` |
| sizing equal to the existing evaluator's | the multiply-accumulate count is the whole path's exactly (`N · K` per output; analytic count asserted), every element of a tiled param is read exactly once, and the residency of a tile is at most `max(tile, widest row)` | `a_decoder_runs_through_tiles_strictly_with_the_same_logits_the_same_work_and_a_bounded_residency` |
| a failing source stops the run with its own refusal | never a silent zero | `a_source_that_fails_mid_run_stops_the_run_with_its_own_refusal` |

Not claimed: that a program with SEVERAL simultaneous faults reports the same §9.3 class first (an element fault is found when its tile is read, not when
the tensor is loaded; §9.3 lets an input that breaks several rules report any one of them). A fault on its own reports the same class and reason. A
second large param on the same node (a `MatMul` of two params) is loaded whole. `eval_cone` (the court's cone evaluation) is not tiled.

**`misaka-palw-sdk::tir_rows::ContainerRowSource`** is that `RowSource` over a `PALWTIR1` container: it reads exactly the bytes of the rows asked for, and
ref2 decodes them with its own `Tensor::from_le_bytes` (only I/O is shared with the first evaluator). `runtime_pack::conformance::run_streamed_tiled_with_progress`
(`StreamTilingV1 { tile_elems, strict, index }`) runs the streamed conformance with the independent implementation tiled; its vectors are the untiled run's
bytes (test `a_streamed_conformance_is_the_loaded_ones_and_never_holds_the_artifact_whole`, three tile sizes with and without the index).

**The stored Merkle index (`misaka-palw-sdk::tir_merkle_index`, `palw-class pack index`).** One 64-byte hash per 32 KiB leaf, in inventory order, in
`<artifact>.merkleidx` (`PALWTMX1`, versioned, with the program binding, the root and a trailer). It is a cache, never an authority: it is believed only after
its leaves fold to the root the caller holds (`verify_root`), and nothing that fails that is used. With it
* a byte range is authenticated by hashing **only the leaves that cover it** (`read_authenticated`) — a row tile of a tensor larger than memory is checked
  against the artifact root without reading the rest (`ContainerRowSource` with an index);
* the leaves a beacon draw names are opened with the consensus multiproof built from the stored hashes, reading those leaves only (`multiproof`);
  `run-conformance` uses a sidecar index exactly this way (`authenticated_openings_auto`) and otherwise makes the streamed pass it always made — a damaged
  index, one for another program, or one that folds to another root is logged and not believed. The evidence is byte for byte the same either way
  (`a_stored_merkle_index_changes_what_the_openings_cost_and_nothing_the_evidence_says`; the measure `open_via_index` and `open_pass_hashed_bytes` say which).
* building the index is the one pass it replaces (`palw-class pack index --artifact <f> [--root <hex>]`; `--root` refuses to write an index that does not fold to
  the registered root).

Not measured: the saving on a real artifact (the tests use the repository's tiny fixtures); the 1.87 GB SmolLM2 class's 3.0 s authentication pass is what an
index replaces per run, at the price of 64 B per leaf (≈ 1.33 M leaves ≈ 85 MB for that class, against 1.87 GB). No fleet or live measurement was made.
