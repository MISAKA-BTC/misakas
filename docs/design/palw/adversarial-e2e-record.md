# Adversarial E2E record — Agent C4 (round 1)

Branch `adv/c4-e2e` (base `717064b16`, plus the Lead's cherry-picks `098ffc749`, `fffaf74ec`, `3d27e026d`, `592ab3382`). Independent of the lanes: every
case below is a new test file, none weakens an existing assertion, no consensus parameter / fingerprint / wire / tag is changed by C4. The first
(only) source change C4 proposed — checked tensor-shape arithmetic — was superseded by the Lead's `098ffc749` (C4's own version was reverted first).

**Top invariant (G14).** Producer + every Panel seat collude; one ordinary public bonded verifier outside the Panel, from canonical public
authenticated material only, reaches objective conviction or a correctly classified DA/default. Plus: relays are never truth authorities, DA
providers never computation authorities, RPC agreement is never a proof, beacons never authorize semantics, heartbeat/BASE-0/EXEC/block hashes
never entropy.

**Level.** Everything here is the **reference level** (the in-process `KernelLedgerV1` fed the strict wire form, the pure contract/tool/library
functions, the client crates). The real-node G14 wiring (lane D phase 2) does not exist yet: every "real node" cell of the G14 matrix is GAP and
is the subject of round 2.

Verdicts: **PASS** (attack refused / correctly classified), **FAIL** (a defect, with the failing test), **GAP** (not expressible yet, with the missing
piece), **OBS** (an observation: behaviour that is by design or economic, recorded so nobody is surprised). A FAIL test is `#[ignore = "FAIL F-C4-n …"]`
and asserts the SAFE property: run it with `-- --ignored` to see it fail. "Fixed" means the Lead's commit made the (un-ignored) test green.

## How to run

```text
export CARGO_BUILD_JOBS=3 CARGO_INCREMENTAL=0
cargo test --offline -p misaka-palw-kernel    --test adv_c4_route            # 27 tests, ~75 s debug  (C4_EXHAUSTIVE=1: all 385 per-node lies, ~340 s; default every 4th)
cargo test --offline -p misaka-palw-kernel    --test adv_c4_fuzz             # ~10.6k mutants + 102,441 every-byte flips of RegisterClass (~100 s)
cargo test --offline -p misaka-palw-kernel    --test adv_c4_fuzz_pipeline    # ~9.2k mutants, 18,969 flips, one-claim, edge/stage lies
cargo test --offline -p misaka-palw-challenge --test adv_c4_contract         # 18 pass, 3 ignored FAIL
CARGO_PROFILE_RELEASE_LTO=off cargo test --offline --release -p misaka-palw-sdk --test adv_c4_conformance   # ~160 s (1,070 evidence flips re-executed)
cargo test --offline -p misaka-palw-remote --test adv_c4_proof --test adv_c4_registration --test adv_c4_transport --test adv_c4_redemption
cargo test ... -- --ignored        # the proven defects still failing
```

## Findings

| id | severity | what | test | status |
|---|---|---|---|---|
| F-C4-01 | P0 | a stranger with one bond panics the kernel-route fold with a tiny object: unchecked products over attacker-chosen tensor shapes (`element_count`, `Tensor::from_le_bytes` `n*width`, `LayoutV1::of` incl. `m*n`), reachable by `FileProof` (Decode logits, Kernel scalar openings), `Respond`; `overflow-checks = true` in release makes it a panic | `adv_c4_fuzz` (3 sites, 113 of 4,982 mutants) | **fixed `098ffc749`**; 0 panics over 10.6k + 9.2k mutants and ~121k byte flips now |
| F-C4-02 | MED-HIGH | a producer's Sybil opens a post-Final demand and stays silent: the forfeit (half to the Sybil, half burned) leaves nothing reserved, and the honest outsider's valid in-horizon proof was dismissed with a fee (effective horizon = `court_deadline_daa`; colluders recoup the accuser share) | `a_post_final_sybil_demand_default_must_not_erase_the_liability_of_a_lying_final_claim` | **fixed `fffaf74ec`** (post-Final forfeit burned whole; a Final claim inside its horizon is adjudicated with nothing reserved) |
| F-C4-03 | HIGH | any other bond re-signs a published claim's evidence for the same job and is paid again (no producer binding, no one-claim-per-job): 2x `claim_reward` for one computation | `a_bond_that_copies_a_published_claim_must_not_be_paid_for_the_same_job_twice` | **fixed `fffaf74ec`** (one claim per job) **+ `3d27e026d`** (seal, then reveal) |
| F-C4-04 | MED (lane D mapping hazard) | a conviction instructed `SlashFraud(reserved)` larger than the collateral the consumer had re-synced lower | `a_conviction_never_instructs_more_than_the_synced_collateral` (+ a two-claim bond cut below the first reservation) | **fixed `fffaf74ec`** (clamp + explicit `ReleaseClaim`); accountant balanced |
| F-C4-09 | MED (DoS) | opened by the first one-claim-per-job fix: a free, shape-correct but unbacked (junk-commitment) claim held the job for the whole check window against the honest producer, at no cost, repeatable with a second bond | `an_unbacked_claim_must_not_be_able_to_lock_a_job_against_the_honest_producer` | **fixed `592ab3382`** (a claim holds its job from the Panel's coverage, not from its commit) |
| F-C4-05 | LOW-MED | `challenge_seed_v1` mints a seed from ANY `WorkBeaconV1` (another epoch's, or an invented output/anchor); only callers that ran `verify_work_beacon_v1` are protected | `adv_c4_contract::a_seed_must_not_be_minted_from_a_beacon_that_is_not_this_contexts` | **OPEN** (ignored); a typed `VerifiedBeacon` returned by the verifier would make misuse unrepresentable |
| F-C4-06 | LOW | `collect_work_beacon_v1` sorts stably on `(settlement, occurrence, canonical_work_id)`: events tying on all three are ordered by arrival (execution commitment / profile not in the key) | `events_tying_on_the_whole_sort_key_must_not_be_ordered_by_arrival` | **OPEN** |
| F-C4-08 | LOW | `verify_conformance_evidence_v1` never reads `failures`: `Passed` evidence that lists failures passes | `passed_evidence_that_lists_failures_must_not_pass` | **OPEN** |
| F-C4-10 | MED | `state_root_of_pinned_header_v1`: the header hash commits `palw_state_root` only for `pow_algo_id` 6,7,8,9 (and only when non-zero); for every other algorithm the root is hash-invisible, so a hostile node can return the pinned block's header with ANY state root and bond/class/claim proofs verify as "PROVEN against pinned block" over the attacker's state (e.g. a client pinned at a legacy/genesis block) | `adv_c4_proof::a_state_root_edited_in_a_header_that_still_hashes_to_the_pin_must_be_refused_for_every_algorithm` | **OPEN**; fix: refuse (`HeaderCommitsNoState`) when the header's algorithm does not hash the root |
| F-C4-11 | MED | beacon-conformance scope flags `require_independent` / `require_backend` are the candidate's own commit-time choice (`--no-require-independent/--no-require-backend`): a reference-only run (`ImplSet{exec:false,ref2:false}`) is `Passed` 30/30 and a verifier mirroring the weak set returns `Verdict::Pass` (a full-set verifier correctly says `EVIDENCE_NOT_REPRODUCED`) | `adv_c4_conformance::a_scope_that_switches_off_the_independent_and_backend_implementations_cannot_pass` | **OPEN**; the flags must come from the approved soundness policy; `Pass` should carry the implementation set |
| F-C4-12 | MED | `verify_signed_registration_v1` reads class/root/owner from the PAYLOAD but never compares them with the file's displayed `class_id`/`artifact_root`/`owner_bond`/`object_digest`; `model submit` (`operator/model_bundle.rs:453`) feeds `s.class_id`/`s.artifact_root` to the registry verdict, the sent-journal path and duplicate detection. A relay editing the file's class to an already-registered one makes `model submit` print "already registered — nothing is sent" and never send the user's registration; filing under another class defeats duplicate detection. Only with explicit `--expect-*` (from-file mode is self-consistent) | `adv_c4_registration::a_signed_registration_whose_displayed_class_root_or_owner_is_not_its_carriers_must_be_refused` | **OPEN**; bind the four fields to the payload in verify, or use `VerifiedSignedV1` in the CLI |

## Observations and residual risks (no defect, recorded)

| id | what |
|---|---|
| O-C4-05 | `RegisterClass` / `PostJob` take no fee at the ledger; a zero-collateral synced bond registers 32 variant classes (distinct `max_positions`) and unlimited jobs; state growth is priced only by the consumer's carrier fee/mass |
| O-C4-06 | the block adjudication budget can be spent by structurally malformed `CommitClaim`s from zero-collateral bonds (charged before the collateral check, no ledger fee): the honest `FileProof` behind them in the same block is `OverBudget` (dropped, state untouched, no fee) and convicts next block |
| R-C4-10 | same-PROMPT copy: a bond posts its own free job with an identical prompt and answers it with the published trace (accepted, covered, 2x reward). Lead: consumer economics (job price >= reward) — not a kernel bug |
| O-C4-09 | a post-Final conviction frees the job and it is paid a second time (the fraud's reward is not clawed back, only the collateral is slashed). Consumer economics |
| R-C4-12 | commit order is not priority: of two correct claims the Panel's coverage order decides the holder; a later copy covered first wins. Inherent unless the squat is re-opened |
| O-C4-13 | lazy seal expiry: objects apply before the tick, so a seal one block past its ttl is still honoured |
| O-C4-14 | an unsealed structurally valid `CommitClaim` spends one adjudication before the seal check (put the seal check first) |
| O-C4-07 | `canonical_work_id` must be domain-separated by the consumer: an earlier non-real event sharing a real work's id suppresses it ("first occurrence counts") |
| O-C4-08 | the policy fixes no minimum `k` (a one-source beacon validates); approval is exact |
| O-C4-11 | the beacon needs `k` distinct work IDS, not `k` distinct executions |
| O-C4-15 | the signer pins class/root/owner only; the builder-chosen `activation_daa`, `share_permille`, `slash_value_per_pwu`, `initial_target`, `pwu_rule` are signed as given (consensus drops the invalid ones: fee lost; a wrong `activation_daa` inside the lookahead just delays the class) |
| O-C4-16 | the multi-RPC quote takes the MAX carrier fee: one lying node of three raises the quoted fee to anything under the user's cap |
| O-C4-17 | `model submit`'s OK row "registered … accepted by a quorum" omits the `UNVERIFIED_REMOTE_STATE` label (the preceding `state` line carries it) |
| G-EXPIRY / G-RULESET | confirmed (known): `expiry_daa`, `quote_digest`, funding maturity fields and `ruleset_id` are editable in the signed file without failing verification |

## Case table — kernel route ledger (reference level)

| attack | test | expected (stated before the run) | verdict |
|---|---|---|---|
| A producer + every Panel seat collude (pre-Final, post-Final, past horizon) | `adv_c4_route::a_producer_and_every_panel_seat_colluding_lose_to_one_ordinary_bond_with_a_fresh_verifier` | one outside bond, replaying the ENCODED blocks, convicts; reward stays paid post-Final; past the horizon dismissed | PASS |
| A' Sybil demand + silence, pre-Final | `a_pre_final_sybil_demand_default_cannot_stop_a_fast_outsider_and_never_yields_a_reward` | fast outsider convicts; slow outsider loses the conviction but the fraud earns nothing | PASS (OBS: a slow outsider cannot convict) |
| A'' Sybil demand + silence, post-Final | F-C4-02 | in-horizon proof still convicts | FAIL -> fixed |
| B self-consistent garbage; C borrowed trace; G input substitution; H last / mid-stream token substitution | `garbage_borrowed_and_substituted_traces_are_all_refused_or_convicted` | refused at inclusion or convicted | PASS |
| D / E / F one fault at EVERY node of position 1 (arithmetic, routing/TopK, history/state, quantize, gather …) | `a_single_lie_at_any_node_of_position_one_is_refused_at_inclusion_or_convicted_and_never_survives_clean` | committed or refused at inclusion, never found clean, convicted with 1000/500; the same proof against the honest claim is dismissed | PASS (385 of 385 with `C4_EXHAUSTIVE=1`) |
| I pipeline edge / stage fault | `adv_c4_fuzz_pipeline::a_pipeline_edge_lie_and_a_stage_lie_are_convicted_and_do_not_touch_the_honest_claim` + lane B's `k2_ledger_pipeline` | convicted; honest claim untouched | PASS (edge + stage; VLM/decode only by lane B) |
| J held / fused terminal | — | the ledger always registers with `ProfileMaterialV1::kernel_route(true)`; held/fused is reachable only through the gate | GAP at ledger level (gate-level in lane B's `k2_public`) |
| K malformed Merkle opening | `adv_c4_fuzz` opening mutants (axis, index, dtype, shape, siblings, values) | dismissed / refused, never a panic, never a conviction of the honest claim | PASS |
| L correct root, material withheld; M fake material | `withheld_material_fake_material_and_a_third_party_server_are_classified_and_never_convict_by_themselves` | transposed shape, other dtype, one element, a row where the whole is owed, another node's tensor, garbage, empty: each rejected with its class; silence defaults (penalty, not slash); a third party's true bytes serve | PASS |
| N court pre-emption; O simultaneous challengers | `court_pre_emption_and_simultaneous_challengers_convict_once_and_pay_once` | one conviction, one reward, two `Duplicate`, 6 sessions moot, bonds return | PASS |
| P challenge at the last block before Final | `a_proof_at_the_last_block_before_final_wins_and_one_block_after_is_a_post_final_conviction` | proof applied before the tick: never Final, no reward issued | PASS |
| Q invalid-challenge spam | `invalid_challenge_spam_is_priced_bounded_and_changes_nothing` + O-C4-06 | 64 dismissed (fee) / 236 over budget, honest lifecycle untouched | PASS |
| R restart mid-prosecution; T duplicate proof after replay | `restart_at_every_block_boundary_mid_prosecution_reaches_the_same_roots_and_a_replayed_proof_is_a_duplicate` | same root after every block from any restart point; replay is `Duplicate` | PASS |
| U bond exit while liability exists | `a_bond_cannot_leave_while_a_claim_or_a_demand_holds_a_reservation` | no withdraw while reserved (claim or demand), still convictable in the horizon, exact withdraw after | PASS |
| seal, then reveal (copyist seals after the reveal; guess seal; re-seal; ttl; exit; wrong signer; unknown job; 3,000 junk seals) | `a_copyist_that_seals_after_seeing_the_reveal_cannot_commit_before_the_original`, `seal_re_seal_expiry_exit_and_signer_edges` | the copy always commits later; the replaced seal opens nothing; exiting bonds neither seal nor reveal; seals swept at ttl+1 | PASS (O-C4-13, O-C4-14) |
| coverage races (two correct claims; refused coverage retried after the holder fails; squatter freed by a demand default) | `observation_the_panel_s_coverage_order_not_the_commit_order_decides_the_holder`, `a_refused_coverage_can_be_retried_after_the_holder_fails`, `a_panel_covered_squatter_is_freed_by_a_demand_default_and_the_honest_producer_then_finalizes` | the first covered holds; retry works; a covered squatter is breakable for a profit | PASS (R-C4-12) |
| codec: signer/actor mismatch | `an_object_signed_by_anyone_but_the_actor_it_names_is_unauthorized_and_changes_nothing` | every named-actor object by any other bond / unknown digest: `Unauthorized`, root unchanged | PASS |
| codec: non-canonical (reordered / duplicated map, Option tag 2, wrong version), tag confusion (every sample under every tag 0..15), length-prefix lies, ceilings of every tag | `non_canonical_encodings_length_lies_and_retagged_objects_are_refused_or_canonical`, `the_decoder_bounds_every_object_before_parsing_and_never_trusts_a_length_prefix` | refused, or equal to its own canonical encoding; never a panic | PASS |
| settlement conservation | `support::Book` (independent) on every block of every test and on every applied fuzz mutant | slash == reward + burn per block; value conserved except `FinalReward` / consumer `sync_bond`; the ledger's bond view == the book | PASS |
| wire fuzz, all 12 route variants + pipeline variants | `adv_c4_fuzz`, `adv_c4_fuzz_pipeline` | I1 no panic, I2 refusal leaves root, I3 honest never convicted, I4 conservation | PASS after F-C4-01 |

## Case table — contract, tool, client crates

| attack | test | verdict |
|---|---|---|
| heartbeat / BASE-0 / EXEC_TX / EXEC slice / receipt-only / provisional / Panel receipt as beacon | `adv_c4_contract::heartbeat_base0_exec_and_receipts_are_never_entropy_…` (adding all 7 kinds at 7 positions, any order, leaves the beacon identical; only-non-real = `Unavailable`) | PASS |
| unfinalized, DA-unsatisfied, validity-dependent, stale-committed, reattached, candidate / dependent work | `unfinalized_stale_reattached_and_self_work_never_become_sources_and_are_never_rescued` | PASS (O-C4-07) |
| candidate self-beacon with a forgetful consumer | `a_forgetful_consumer_cannot_let_a_conformance_candidate_seed_its_own_challenge` | PASS for MODEL/KERNEL conformance (`candidate_profile_id` required, candidate excluded even if the list is empty); **GAP-C4-A** for CLAIM_VERIFICATION / WORK_SLICE / PUBLIC_PROSECUTION: self-exclusion is the consumer's `excluded_profiles` |
| Panel-licensed source into PANEL_ASSIGNMENT | `a_panel_licensed_final_never_seeds_a_panel_assignment` | PASS |
| duplicate / reattached work; contribution reorder | `every_permutation_of_the_same_events_gives_the_same_beacon` (720 permutations) | PASS (F-C4-06 for exact key ties) |
| reorg before / after lock | `a_reorg_before_the_lock_changes_the_beacon_and_one_after_the_k_th_source_does_not` | PASS |
| forged / edited presented beacon (every source field, accumulators, output, anchor, lock, order, duplicate, missing) | `every_single_field_edit_of_a_presented_beacon_is_refused` | PASS |
| seed reuse across subject kinds / subject fields | `six_subject_kinds_six_seeds_and_no_single_subject_field_is_a_free_edit` | PASS |
| sampler bias (fixed seed, generous bounds) | `the_index_sampler_is_uniform_…` (chi-square n=3,10,1000; n=2^63+1 upper half 0.48–0.52), `the_distinct_sampler_…` (all C(10,3) subsets, 0.28–0.32), `the_field_sampler_…` (never p, top bit ~50 %, bound => error) | PASS |
| transcript round substitution (staged / Fiat–Shamir) | `transcript_rounds_cannot_be_substituted_reordered_or_rebound` | PASS; **GAP-C4-B**: `FiatShamirTranscriptV1::verify` does not know the expected statement/policy/beacon — the consumer must compare the header |
| extreme positions / counts never panic | `extreme_positions_windows_and_counts_never_panic_the_beacon` (≈ 650 contexts) | PASS |
| onboarding lifecycle shortcuts | `the_onboarding_lifecycle_has_no_shortcut_to_active` (model-based) | PASS; **GAP-C4-C**: failed conformance re-committed without limit (60 failures, then pass) — no failure counter / retry limit in the record |
| conformance evidence forgery (20 single-field forgeries, SKIPPED/INCOMPLETE/FAILED/UNAVAILABLE with passing counters, stale seed, other commitment) | `conformance_evidence_is_recomputed_…` | PASS (F-C4-08) |
| beacon-conformance tool: every single-byte flip of valid evidence, with re-execution | `adv_c4_conformance::every_single_byte_flip_of_valid_evidence_is_never_a_pass` (1,070 bytes) | PASS |
| stale / short / other-history facts (tip before lock, extra earlier source, changed execution commitment, dropped source, moved epoch / commitment position) | `stale_short_or_different_facts_never_verify_evidence_…` | PASS |
| facts-file fuzz | `mutated_facts_files_never_panic_…` (618 parsing mutants) | PASS: only `final_path.panel_seed_id/panel_epoch` and `tip_position` are ignorable |
| evidence of one commitment under another | `evidence_of_one_commitment_never_verifies_under_another` | PASS |
| artifact / layout / plan / policy substitution after commit; `--no-rerun`; SKIPPED-as-PASS | lane C's `runtime_pack_beacon` (not re-run) | covered by lane C |
| candidate-chosen scope strength | F-C4-11; `gap_a_tiny_scope_with_a_declared_total_fault_density_buys_forty_bits` (GAP-C4-D: 30 x (1 vector of 1 token + 1 leaf) derives >= 43 bits); O-C4-11 | FAIL / GAP |
| state-proof forgery: edited header (every algorithm), preimage byte, label, row value / key, truncated / dropped / duplicated / reordered / extra row, hidden row proving absence, other state under our pin | `adv_c4_proof` (4 tests) | PASS except F-C4-10 |
| state-proof file form fuzz | `the_state_proof_file_form_never_panics_on_garbage` | PASS |
| detached registration: any edit of any JSON leaf (79) or carrier bit (17,380 flips) of a signed registration | `a_relay_that_edits_any_field_or_any_carrier_byte_of_a_signed_registration_changes_nothing_or_is_refused` | PASS for the carrier; edits of displayed fields accepted (F-C4-12), unsigned `expiry_daa` etc. (G-EXPIRY) |
| unsigned bundle: 196 leaf edits then sign | `observation_the_signer_signs_whatever_object_fields_…` | a builder can change the funding outpoint and the unpinned object fields (O-C4-15); class / root / owner / fee / recipient never |
| multi-RPC disagreement | `quote_agreement_stops_on_any_identity_difference_and_merges_the_rest_to_the_worst_case` (13 identity fields) | PASS (O-C4-16) |
| duplicate submission vs resend | `resend_versus_duplicate_matches_an_independent_oracle_over_every_combination` (3 verdicts x 8 journals) | PASS |
| malicious relay replies; hostile providers (flipped / swapped / truncated / padded chunks, wrong-claim manifests, silence); hostile redemption listing (traversal, wrong length, empty); hostile HTTP peer (huge Content-Length, endless body, unterminated header, absurd chunked size, garbage, redirect) | `adv_c4_transport` (3 tests) | PASS |
| redemption: duplicate quantum range edges, fee at / above the cap, `u16::MAX`, empty / inverted range, other network / version / beacon rule, every field edited after signing, every byte flip of an `RDA4` bundle, split conservation over 110 (part, bps) pairs | `adv_c4_redemption` (3 tests) | PASS |

## GAP — not expressible yet (round 2 and later)

| # | what is missing |
|---|---|
| 1 | every **real-node** G14 cell: `KernelRouteObjectV2`-style carrier + acceptance arms + fold + per-block tick + bonds mapped onto `PalwBondKeyV2` + RPC reads (claim record, served positions, demands, verdicts, public material). Round 2 re-runs this record's kernel cases from a fresh verifier process against `T12Chain`, plus restart / IBD / reorg equality of the kernel-route root |
| 2 | real-processor redemption cases (payout to a non-registered destination, two builders racing, reorg after payout, V3 unchanged pre-fence): lane D's `rfc9_redemption_v4` exists; C4 re-ran only the pure primitives |
| 3 | real-node registration attacks beyond lane D's phase 1 (it is already broad: parity table, replay, restart, IBD, reorg) |
| 4 | EXEC work slices (RFC-0008 v2): no code on HEAD |
| 5 | the RPC wire (`getPalwStateProof` op 202, the registration reads) over a running service — exercised only through their functions |
| 6 | RFC-0010 Panel assignment, RFC-0001 and the VM lanes: not integrated |
| 7 | J held / fused terminals at ledger level; VLM / decode pipeline semantic attacks (lane B only) |

## Commits

`dfca3469d` fuzz (F-C4-01 reproduction) · `254018073` route E2E · `f49c97e4c` widened fuzz, pipeline, every-byte flips · `e655de052` re-verification of F-C4-02/03/04 and
the squat · `a310c9c3c` challenge contract · `5093f4446` seal / coverage · `35d702c93`, `7ea602cc0` conformance tool, proof forgery, registration, transport, redemption · this record.

---

# Round 3 — Agent C4r3, the REAL node (2026-10-08)

Branch `adv/c4r3-real-node` (base `183a761bf`, the Lead's integration HEAD). Rounds 1–2 attacked the reference crates (above; their
body is copied from `adv/c4-e2e@21b83fab2`). Round 3 attacks what landed on the node since: the kernel route in `PalwChainStateV2`
(tags 110/111, `SealClaim`, proof grace, post-Final liability, block budget, RPC 210–212), RFC-0015 OPV (Panel = 0), the onboarding
objects (104–108), and reads the RFC-0010 V3 fold and the single challenge contract. Every PoC runs the real path: signed `0x4b`
carrier → mempool → the node's template → chain-block fold → read API, in `consensus/src/pipeline/virtual_processor/tests/
g14_kernel_route_e2e/c4r3.rs` (a child of the lane's harness, `g14_c4r3_*`) and `consensus/core/tests/c4r3_fork_id_at_int13.rs`.
Convention as in round 1: a defect's test is `#[ignore = "FAIL F-C4R3-nn …"]` and asserts the SAFE property; `observation` tests pin
by-design or already-named behaviour, quantified.

**Nothing here touches what DAA 9,000 arms.** The int-13 list (`PALW_T12_INT13_FENCES_V1`: `palw_audit_1004_v1`,
`palw_gen_range_twin_v1`, `palw_model_court_window`, `palw_receipt_spend_v4`) arms none of the G14 fences; every G14 object is dropped
by name, first and charged nothing, below `palw_probabilistic_constraints_v1` (the acceptance walk), the G14 carriage tails are written
only when their state exists, and the only consensus reads of `consensus_params_id` are in dormant G14 rules (F-C4R3-01). F-C4R3-04
is a release-process hazard OF the 9,000 height.

## How to run

```text
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0
cargo test --offline -p kaspa-consensus --lib g14_c4r3                    # the passing PoCs, controls and observations
cargo test --offline -p kaspa-consensus --lib g14_c4r3 -- --ignored       # the proven defects (fail on this revision)
cargo test --offline -p kaspa-consensus-core --test c4r3_fork_id_at_int13
```

## Findings

| id | sev | what | PoC | root cause | status |
|---|---|---|---|---|---|
| F-C4R3-01 | **P0 where armed** (latent: the fences are refused at every real height) | **Consensus rules that bind `Params::consensus_params_id` split upgraded from un-upgraded nodes at the binary swap, not at any height.** `consensus_params_id` hashes every SCHEDULED fence (the int-13 re-pin moved testnet-12's `5ee7fd8e…` → `2e567642…` with nothing in force changing). (a) The kernel route's ledger policy carries `ruleset_digest = consensus_params_id`, the route header stores it, and `ensure_route_header` refuses a stored route "folded under another policy": the first node to take ANY later release disqualifies every block with a live claim or demand (measured: `is disqualified from virtual chain (PALW state): a kernel route move is refused: the stored kernel route was folded under another policy`), while un-upgraded nodes accept it — and two builds rolled out across the route's creation create different headers. (b) The G-RULESET envelope (tag 108) must name the same id: an envelope in flight during a rollout registers on old nodes and is dropped by upgraded ones; the next chain block commits the old build's root and the upgraded node disqualifies it (measured: sinks differ, PALW roots differ). The handshake keeps both builds as peers (same identity, same fork id below the new fence) — by design (M1-6). | `g14_c4r3_a_release_that_schedules_an_unrelated_fence_keeps_the_route_folding` (restart over the same DB as the next release); `g14_c4r3_an_envelope_in_flight_must_not_split_builds_that_differ_only_in_a_future_fence` (ignored FAIL) | `consensus/src/pipeline/virtual_processor/processor.rs:1193` (`palw_native_ruleset_id = consensus_params_id()`), used at `:14988` (route extras, before the fix) and `:14906` (envelope); `consensus/core/src/palw_kernel_route_fold_v1.rs:168` | **(a) fixed locally `0fc4adf0f`** (`fix(F-C4R3-01)`: the route's digest is `consensus_identity_id()`, +7 −1 in `processor.rs`; the PoC is green). **(b) OPEN** — the envelope's field is named `consensus_params_id`; proposal: name the ruleset IN FORCE at the block, `fork_id_v1(params, daa).fired` (genesis + fired heights: equal across builds that differ only in future fences, moves at every crossed fence), or the identity plus `valid_until_daa`. Same root cause, local only: `processor.rs:3327/3513` and `consensus/src/consensus/mod.rs:2918` compare the persisted native-settlement snapshot's `ruleset_id` with it (dormant DNS retirement; after an upgrade the EVM canonical heads stop until a resync) |
| F-C4R3-02 | **P1** | **A provable fraud is laundered into a free availability default by keeping ONE valid proof out of the chain for `court_deadline_daa` (20 DAA), not for the window plus the liability horizon (250 DAA).** The producer publishes everything; the colluders' own bond demands any position and the producer stays silent. At the deadline the claim is `Unavailable`: the producer pays `default_penalty` and the rest of the reservation is released; pre-Final the demanders take the WHOLE penalty (Panel-licensed) or 90 % of it (OPV, 10 % burned). The outsider's true proof, filed next, is **dismissed with the filing fee** ("nothing is reserved any more"). Measured (Panel / OPV): censored 21 DAA; producer −100 KAS; colluding demander +100 / +90 KAS; outsider −0.1 KAS; not convicted. Round 1's O-level "slow outsider" (A') on the node: the colluders' cost is 0 KAS (Panel) / 10 KAS (OPV) against a 1,000 KAS slash, and RFC-0015 §6.3's censorship cost (saturate the court through window + liability) overstates the requirement ~12×. | `g14_c4r3_a_self_inflicted_default_must_not_erase_a_provable_fraud`, `g14_c4r3_opv_…` (ignored FAIL) | `misaka-palw-kernel/src/ledger.rs:1789` (pre-Final default paid to demanders), `:1847` (an `Unavailable` claim's reservation released at once), `:1552` (a proof against it dismissed with the fee) | OPEN — proposal (lane B, changes the existing default assertions, so not a local fix): a pre-Final default takes the penalty but KEEPS the rest reserved until `default + liability_daa` (set `liability_until`), during which a valid proof convicts it (the post-Final rule already does this with nothing reserved); burn a share of a Panel-mode default as OPV does; never charge the fee for a proof against a claim whose material was published |
| F-C4R3-03 | **P1 where armed** (the chunk lane itself is armed on testnet-12 for `FamilyCertified`; that griefing is ADR-0075 SA-1's named residual) | **Eight junk chunks (1.6 KAS of rent) hold every chunked prosecution off the chain for 4,000 DAA.** A kernel object larger than one carrier — a real class's `FileProof`, a position `Respond`, an onboarding refutation (105) — rides `ObjectChunk`s; the chunk lane is ONE network-wide table of 8 half-assembled groups held until a 4,000-DAA TTL; a chunk is unsigned and anyone may open a group at the flat 0.2 KAS slot rent. Measured: 8 one-part junk groups; the outsider's chunked proof (cut at 1 KiB to stand for a real class's) re-sent six times between DAA 4 and the horizon (254) never opens a group; the lie finalizes at 54 and leaves its liability horizon unconvicted (control: the same chunked proof convicts when the lane is free). The same 1.6 KAS makes an honest producer default on a demanded position whose response needs chunks (and a pre-Final default pays the demander), and keeps a false artifact binding unrefuted until it is Final (window 40, horizon 200). | `g14_c4r3_eight_junk_chunk_groups_must_not_hold_a_chunked_proof_off_the_chain` (ignored FAIL); `g14_c4r3_control_a_chunked_proof_convicts_when_the_chunk_lane_is_free` | `consensus/core/src/palw_state_v2.rs:9103-9105` (8 groups, TTL 4,000), `:35585` (a new group refused when full), `:9304` (kernel / onboarding objects admitted to the same lane) | OPEN — proposal: G14 objects do not share the certification lane: a separate chunk table whose groups are opened by a SIGNED chunk of an Active bond (one open group per bond, the rent a bond-backed deposit forfeited at TTL), and a TTL no longer than the object's own deadline (a proof group that cannot complete before the claim's horizon is worthless); or raise the carrier so every worst-case filing/response of an admitted class fits one (then `carrier_fit_v1` with the single-carrier cap) |
| F-C4R3-04 | P2 (release process, the 9,000 height) | **A fence that joins an already-scheduled height is invisible to the fork id.** The int-13 list's own doc invites the unready G14 lanes to "join by one line here plus the re-pin". Once a fleet runs the int-13 build, a later build adding `palw_probabilistic_constraints_v1`, `palw_panel_free_v1` or `palw_signed_registration_v1` at 9,000 has the same fork id at every height and neither side refuses the other past 9,000, where they disagree about blocks (schedule-id warning only). | `c4r3_observation_a_fence_joining_the_int13_height_leaves_the_fork_id_unchanged` (passes: pins the hazard) | `consensus/core/src/fork_id_v1.rs:298` (heights only); `consensus/core/src/config/params.rs:21880` | OPEN — freeze `PALW_T12_INT13_FENCES_V1` (a pin test on its names) once its build is deployed; a lane that misses it takes a fresh height |
| F-C4R3-05 | P2 (DoS, OPV) | **OPV lane capture.** `max_live_claims_total` is one network-wide counter; a claim counts while its reservation is held — through the window AND the liability horizon (≈ 250 DAA at the interim terms) — and jobs are free. ⌈32 / 3⌉ = 11 bonds fill the lane with HONEST claims on jobs they posted themselves; every other producer's OPV claim is refused while they refill, and the occupiers are paid the (unfunded) `claim_reward` at each Final. Measured with the cap scaled to the harness's eight cards (total 6, per producer 3): two bonds fill it, a third producer's honest claim is refused; an occupier's slot frees at Final (55) + liability (200). | `g14_c4r3_opv_two_bonds_must_not_be_able_to_hold_the_whole_opv_lane` (ignored FAIL) | `misaka-palw-kernel/src/opv.rs:560` (global cap), `:512` (live = reserved > 0, liability phase included); `consensus/core/src/palw_panel_free_v1.rs:75` | OPEN — count only pre-Final claims toward the admission cap (a Final claim's liability needs its reservation, not an admission slot), cap per job poster / class, and price admission with a non-refundable fee |

## Observations (no new defect; quantified)

| id | what |
|---|---|
| O-C4R3-R7 | **GAP-R7 quantified on the real path** (`g14_c4r3_observation_gap_r7_a_lifted_proof_takes_the_bounty_and_halves_the_colluders_loss`): a `FileProof`'s proof bytes name no accuser; the proof is lifted from the honest carrier's PUBLIC bytes, re-signed by another bond and included first. The copyist is paid the whole bounty (500 KAS = half the reservation), the outsider's filing is a `Duplicate` (0 KAS, no fee). With the producer's own bond as the copyist the lie costs the colluders HALF its reservation; under any rational block producer an honest verifier's expected bounty is ≈ 0. Also: `OpvPolicyV1::required_reservation` (`opv.rs:150`) does not divide by `1 − accuser_reward_permille/1000`, so the self-recoup halves the margin the relation claims (the interim 1,000 KAS still clears the 120 KAS it computes). The accuser seal is the fix (open by decision). |
| O-C4R3-mint | GAP-5 reconfirmed with its rate: `SettlementKindV1::FinalReward` is "newly issued" (`settle.rs:16`) and the node queues it to the coinbase; `PostJob` is free, so a bond answering its own jobs mints `claim_reward` per job — unbounded on the Panel route, ≤ 32 live claims on OPV. Blocking for any activation. |
| O-C4R3-respond | `Respond` is signed by ANY synced bond (no named actor) and a rejected response spends one adjudication of the block's budget with no fee (`ledger.rs:1732`): the budget can be spent free while a demand is open. Bounded in production by the block's carrier count (~125 KB of transactions a block), so not a censorship primitive at 64 adjudications; it does break §6.3's "each junk run forfeits the filing fee" premise. P3. |
| O-C4R3-root | A header commits its SELECTED PARENT's PALW root, so a fold divergence between builds surfaces one chain block later (F-C4R3-01b's PoC mines that block). By design; recorded because a divergence in the sink block is invisible until a child exists. |
| O-C4R3-drop | A kernel object the ledger refuses is "dropped", yet the arm still writes the route header on first use and the block's budget row (`palw_kernel_route_fold_v1.rs` refusal path): deterministic, but the module doc's "nothing is written" is not literal. |
| O-C4R3-v3 | RFC-0010's receipt-clock pause (`palw_panel_v3_fold_v1.rs:310`, the structural guard the Lead asked for) pauses on ANY open DA session or court session and re-bases at the last close — read, holds. Its residual: the 16-session lifetime cap can be spent by the colluders' bonds, but each refuted session costs `r × S_P` (DA-6), so 16 of them cost more than the claim's reward; and every V3 draw ends `BeaconUnavailable` today (`approved_beacons` is empty in every release), so no V3 claim binds. Not exercised on the node by C4r3. |

## Held (negative results)

| attack | evidence | verdict |
|---|---|---|
| **Mandatory test 3, Panel-licensed, node-less**: the producer's signed seal and lying reveal relayed by `misaka_palw_remote::relay::broadcast_signed_tx` (one relay lies and is reported `Tampered`, never counted); every seat signs a passing receipt; an ordinary bonded outsider (read API + public DA, own salt) relays its proof the same way | `g14_c4r3_mandatory_3_a_relayed_lie_covered_by_every_seat_is_convicted_by_an_outsider` | PASS: convicted, the REAL producer bond −1,000 KAS, outsider +500, seats untouched, replay equal |
| **Mandatory test 3, OPV (Panel = 0), node-less**, before and after Final | `g14_c4r3_mandatory_3_opv_relayed_lies_are_convicted_before_and_after_final` | PASS: one convicted in the window, one Final then convicted inside liability; both reservations slashed; replay equal |
| a chunked proof when the chunk lane is free | `g14_c4r3_control_a_chunked_proof_convicts_when_the_chunk_lane_is_free` | PASS |
| F-C4-05 (seed from any beacon) | `VerifiedWorkBeaconV1` has private fields, no decode, built only by the collector/verifier; `challenge_seed_v1` takes it and checks the context id (`misaka-palw-challenge/src/beacon.rs:209`, `seed.rs:30`) | fixed (inspection) |
| arming a G14 fence on a real network | `validate_palw_v2` refuses `palw_probabilistic_constraints_v1`, `palw_panel_free_v1` (value checked first), `palw_signed_registration_v1` at every height; `OverrideParams` has no PALW field (`--override-params-file` cannot arm them; refused on mainnet anyway); no drill flag names them | holds (inspection) |
| G14 objects on testnet-12 below the fence | dropped by name, first and charged nothing, before any slot / rent / counter (`palw_v2_accepted_objects_and_refunds`), matching an older build's undecodable skip (A-2); a chunk group completing into a dormant G14 inner is dropped on both builds (the gate here, `ChunkedObjectUndecodable` there) | holds (inspection) |
| slash clamping / a vanished producer bond | the producer is re-synced before every proof/demand/response and the tick (`kernel reserved ≤ synced ≤ collateral`); `slash_bond` debits `min(amount, collateral)`; V2's exit gates read the kernel and onboarding reservations, so a producer bond with a reservation cannot leave | holds (inspection; lane tests cover the exit) |
| onboarding refutation misjudged | the kernel opening's dtype and shape are inside its commitment (`merkle.rs:248`), rows are contiguous last-axis rows, the byte comparison is two's-complement over `i128` values at the declared width, the V2 leaf is pinned to the layout's index and count; the instance-set proof is exact equality with the program's declared instances | holds (inspection) |
| baseline | the lane's 50 `g14_` tests at `183a761bf`; then all 60 `g14_` tests (lane + C4r3) after `fix(F-C4R3-01)` | 50/50 pass; after the fix 54 pass, 5 ignored (the open FAILs), 1 failed: `g14_registration_replay_on_a_second_node_and_across_a_reorg` — lane D's own GAP 11 (no strict-win fence: the reorg is a tie decided by candidate hash). It failed 3× in a row under machine load and passed 3/3 later on the same source, and passed once with the fix reverted: a flake of that race, not the fix (which is read only where the route's fence is in force; that test never arms it). Worth arming the strict-win fence in that test. |

## Not attacked in round 3

RFC-0010's V3 fold on the node (beyond reading the pause; V3 cannot bind without an approved beacon); pipeline claims on the node (no
Panel, no header wire form); a cross-lane slash race (a V2 conviction draining the producer bond to a colluder before the kernel's,
F-C4-04's clamp then shrinks the honest bounty); the O(rows) per-object fold cost (GAP 8) as a DoS figure; reorg across Final with
sibling blocks in the DAG; beacon grinding (no approved scheme to grind).

## Commits (C4r3)

`654c89ace` PoCs · `d0c1c97d9` F-C4R3-01 (envelope) PoC · `0fc4adf0f` **fix(F-C4R3-01)** · `6af3b894d` F-C4R3-03 PoC + control · this
record.
