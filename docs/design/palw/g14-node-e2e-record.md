# Kernel route on the real node — E2E record (G14 lane D, phases 2, 2b and 3)

Branch `g14/d-node-e2e`. The probabilistic-constraint route (`misaka-palw-kernel`, ADR-0172 / RFC-0011) and its RFC-0015
`OptimisticPublicVerification` (OPV) mode are folded into `PalwChainStateV2` and exercised by
`consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e.rs` on the real testnet-12 harness: signed `0x4b` carriers from the
genesis float, the mempool check, blocks built by the node's own template, the chain block's fold, the persisted tip and per-block delta
rows, `ConsensusApi` reads. Status is **PASS only where a test exercises the real path**; anything the current code cannot express is
**GAP** with the exact missing object or state.

```text
signed object -> 0x4b carrier -> mempool -> node template -> acceptance walk (fence, signature, strict decode) -> chain-block fold
  -> kernel ledger (rows) -> settlements on the REAL bonds / coinbase queue -> tip + delta rows -> ConsensusApi / RPC 210-212
fresh verifier = the read API's rows + a public DA directory, its own salt -> FileProof / FileDemand -> the same path
```

## 0. Reproduce

```text
export CARGO_BUILD_JOBS=3 CARGO_INCREMENTAL=0
cargo test --offline -p kaspa-consensus --lib g14_kernel_route      # 18 route cases, ~40 s
cargo test --offline -p kaspa-consensus --lib g14_opv               # 8 OPV cases, ~10 s
cargo test --offline -p kaspa-consensus --lib g14_onboarding        # 5 onboarding cases, ~25 s
cargo test --offline -p kaspa-consensus-core --test rfc0015_panel_free   # both OPV fences
cargo test --offline -p misaka-palw-kernel --test k2_rows           # rows view, both root forms
cargo test --offline -p kaspa-rpc-core --lib palw_kernel            # ops 210-212 wire round trips
cargo test --offline -p kaspa-grpc-core --lib palw_kernel_route_grpc
```

## 1. What is wired (frozen allocation)

| Item | Value |
|---|---|
| Consensus objects | tag **110** `KernelRouteV1 { bytes, signer, signature }` (the kernel's strict-encoded object, signed by a V2 bond, ML-DSA-87 over `H(network ‖ signer ‖ bytes)`); tag **111** `KernelConstraintReceiptV1` (a seat's signed receipt) |
| Delta entries | **160** `KernelRouteRow { table, key, old, new }`, **161** `KernelRouteHeader { old, new }` |
| Carriage tail | **0xEC** (the route's header, rows and aux rows; pruned import reproduces the root) |
| Root | Some-only block `kernel-route/v1` = ledger root ‖ aux root. Ledger root = the kernel's historical root form, or `StateRootPartsV2` once an OPV policy is in the header |
| Rows | ledger tables 1–14 (bonds, classes, pipeline classes, jobs, pipeline jobs, claims, demands, served, attested, job-claims, seals, OPV admitted / classes / claims); consensus tables 32 bond keys, 33 interim assignments, 34 counted receipts, 35 block budget |
| Fence | `Params::palw_probabilistic_constraints_v1` — validation REFUSES every real height; the harness builds `Config` directly (and asserts the refusal still holds) |
| Reads | `ConsensusApi::palw_kernel_route_v1()`; RPC **210** `getPalwKernelClaim`, **211** `getPalwKernelRows` (paged), **212** `getPalwKernelFinals` |
| Mode (RFC-0015) | tags 13 / 14 ride inside tag-110 bytes; dropped at the gate unless the OPV fence is in force; OPV policy = genesis constant in the route header |

Fold shape: **eager per object** — each object loads the ledger from the rows, runs the kernel's own per-object API, applies every
`SettlementInstructionV1` exactly, writes back the rows that changed. A kernel refusal DROPS the object (nothing written, block stands);
only a missing fence fails a block. The closing tick (`tick_kernel_route_v1`) runs the demand deadlines, windows, Final and liability
release, and is lenient in its settlements (it runs after the rehearsal, so an error there would fail a block). Before an object is applied
the ledger is told the real collateral of the signer and of the producer of the claim it names, net of every non-kernel reservation
(`kernel reserved ≤ synced ≤ collateral`), so a slash never clamps and the accuser / demander / burn split conserves. Slashes are
`slash_bond` (burn at release); rewards, demander shares and the Final reward join the coinbase queue under prefix `0xFD`. A kernel
`Withdraw` is the route forgetting the bond, never a V2 exit. V2's committed-collateral ledger and both withdrawal gates read the
kernel's reservation.

The per-block adjudication budget bounds the BLOCK across eager objects: what earlier objects of the same chain block spent is persisted
(aux table 35) and restored, and a refusal that charged still counts (tests run with a 4-adjudication block, `cfg(test)`).

## 2. Cases

| Case | Path | Test | Status |
|---|---|---|---|
| Covered lie (producer + every interim seat colluded) convicted by an outsider outside the Panel, fresh verifier from read-API rows + public DA | register -> job -> seal -> reveal -> assignment -> 3 receipts -> proof -> slash -> coinbase payout | `g14_kernel_route_a_covered_lie_is_convicted_by_an_outsider_through_the_real_path` | PASS |
| Same, after Final, inside the liability horizon | Final reward queued, then conviction | `..._convicted_after_final_within_the_liability_horizon` | PASS |
| Withheld position -> signed demand -> no response -> default (availability, never fraud) | demand bond reserved on the real bond; penalty slash; demander paid | `..._a_withheld_position_is_a_demand_then_a_default_never_a_conviction` | PASS |
| Malformed / truncated / wrong bytes / wrong root / fake opening responses | all rejected, demand stays open, then default | `..._responses_are_rejected_then_the_producer_defaults` | PASS |
| A real response completes the check and convicts; a served position cannot be demanded again | | `..._a_served_position_completes_the_check_and_convicts` | PASS |
| Court pre-emption: five spam demand sessions never pre-empt a direct proof; all settle moot | | `..._spam_demands_never_preempt_a_direct_proof_and_settle_moot` | PASS |
| Simultaneous challengers: one conviction, one Duplicate, one slash, one reward | same block, two cards | `..._simultaneous_challengers_...` | PASS |
| Duplicate proof refiled on a replaying node | no second slash / reward | `..._a_duplicate_proof_after_a_replay_changes_nothing` | PASS |
| Proof grace: served at the window end, Final waits, the enabled proof convicts BEFORE Final | | `..._final_waits_the_proof_grace_...` | PASS |
| Spam cannot hold Final past window end + court deadline + grace | 10 demands, served at deadlines | `..._spam_demands_cannot_hold_final_past_...` | PASS |
| Bond exit with liability refused; released at the horizon | kernel `RequestExit` / `Withdraw`, V2 duty gate | `..._a_bond_with_liability_cannot_exit_until_the_horizon_releases_it` | PASS |
| Large object via `ObjectChunk`: signature checked on the assembled whole at the completing chunk; tampered => completing chunk dropped; not a certification for the grading cap | 4 KiB chunks | `..._a_chunked_object_is_signature_checked_at_the_completing_chunk` | PASS |
| Replay on a second node; reorg to a heavier branch (rows/aux/collateral/queue restored EXACTLY) and back | | `..._replay_and_reorg_reach_the_same_roots` | PASS |
| Real restart over the same database; carries on; replay agrees | | `..._survives_a_node_restart_over_the_same_database` | PASS |
| Pruned import with live rows (assignment, seats) through tail 0xEC; importer folds to every root | | `..._survives_a_pruned_import` | PASS |
| Block adjudication budget bounds the block across objects; refusals still spend it | 5 junk registrations in one block | `..._the_block_adjudication_budget_bounds_the_block_not_each_object` | PASS |
| Read model: claim read, served/demand rows, rows paged and rebuilt to the committed root, public record builds a fresh verifier | | `..._the_read_model_serves_a_claim_and_rows_that_rebuild_the_committed_root` | PASS |
| Hostile signature-valid objects (junk proofs, `[u64::MAX, 2]` shape, stage 255 / position `u32::MAX`, unknown claim/job/bond) dropped or dismissed; the filer pays the dismissal fee as a real slash; chain carries on; genuine proof still convicts | | `..._hostile_objects_are_dropped_or_dismissed_and_never_stop_the_chain` | PASS |
| **OPV** honest claim: Challengeable from inclusion, nothing passes it early, Final at the window end with NO Panel; the beacon fact is `FinalPathV1::PanelIndependent` | | `g14_opv_an_honest_claim_finalizes_...` | PASS |
| OPV lying claim convicted pre-Final by a fresh outsider; nothing finalizes or pays it | | `g14_opv_a_lying_claim_is_convicted_...` | PASS |
| OPV lie that finalized convicted within liability; its fact flips to `claim_final = false`, `ConvictedAfterFinal` | | `g14_opv_a_lie_that_finalized_...` | PASS |
| OPV withheld material: default, 10% of the penalty burned, the squatted job freed for an honest claim | | `g14_opv_withheld_material_defaults_...` | PASS |
| OPV spam bound: Final within the hard deadline | | `g14_opv_spam_demands_cannot_hold_final_...` | PASS |
| OPV registration dropped without the fence / unadmitted class / PanelLicensed by tag 13; legacy class coexists with a Panel-covered conviction | | `g14_opv_registration_is_dropped_...` | PASS |
| OPV reorg (rows and aux exactly restored) and restart (finalizes after it) | | `g14_opv_replay_and_reorg_...`, `g14_opv_survives_a_node_restart_...` | PASS |

## 3. Harness seams (all `cfg(test)`, none in a build that can run a network)

| Seam | Why |
|---|---|
| `Config::new(params)` instead of `ConfigBuilder::build()` | `validate_palw_v2` refuses `palw_probabilistic_constraints_v1` at every height by design |
| `kernel_route_test_attest_artifact_v1(root, from_daa)` | the phase-2 route cases register their kernel class over an artifact nobody bound; phase 3's real source (§5) replaces it, and the onboarding cases never call it (their weights are not in its list) |
| `extras.max_adjudications_per_block = Some(4)` | reaches the cross-object budget with five carriers instead of sixty-five |
| `params.palw_reorg_strict_economic_win = 0` in the harness config | a heartbeat branch against a heartbeat branch ties on every economic key; without the fence the V2 deep-reorg gate decides the tie by candidate hash (a race) |

## 4. Phase 2b — RFC-0015 OPV: the policy's home

`Params::palw_panel_free_v1` is now `Option<PalwPanelFreeFenceV1 { activation, admitted_classes (strictly ascending), window, budgets,
economics, carrier }>` (Some-only hashed, `never()` collapsed, `validate_palw_panel_free_v1` refuses every armed height and checks the
value first). The processor derives `extras.opv` — the genesis OPV policy (`fence.opv_policy()`, activating at the fence's height) and the
network's admission list — from it; the cfg(test) seams of phase 2b are gone. `PalwPanelFreeFenceV1::interim_v1` holds the interim terms;
tests arm it with `Config::new` (the real validation refuses it by design).

## 5. Phase 3 — model onboarding on the real node

All dormant behind `palw_probabilistic_constraints_v1` (the envelope behind its own `palw_signed_registration_v1`); presets unchanged.

| Item | Value |
|---|---|
| Objects | **104** `ArtifactBoundV1`, **105** `ArtifactBindingChallengedV1`, **106** `KernelBoundV1`, **107** `ConformanceCommittedV1`, **108** `SignedRegistrationV1` (109: OB-P0's conformance evidence, §7). Each signed by a V2 bond (ML-DSA-87 over `H(network ‖ kind ‖ signer ‖ payload)`, `palw_onboarding_message_v1`) |
| Rows | the route's aux tables **36** artifact bindings `(class, kernel root)`, **37** kernel bindings `class`, **38** conformance `(class, artifact root)`. They ride the route's existing `KernelRouteRow` deltas (160), tail `0xEC` and `kernel-route/v1` root block — **no new delta, tail or root block** (the allocated 190–199 / `0xEF` stay unused: the onboarding rows are the route's state, journaled by the one writer that already reverts, roots and carries them) |
| Reservation | a binding holds `PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1` of the binder's FREE collateral until its liability horizon ends, mirrored into V2's committed-collateral ledger and both withdrawal gates (`onboarding_reserved`) |
| Fence | `Params::palw_signed_registration_v1` (tag 108): dormant, `None` everywhere, Some-only hashed, `never()` collapsed, refused when armed |
| Large objects | the refutation (a commitment map + two openings) rides `ObjectChunk`s like a kernel object: judged on the assembled whole at the completing chunk, and not a certification for the grading cap |
| Read | `ConsensusApi::palw_onboarding_v1`, RPC **230** `getPalwOnboarding` (bindings with state, kernel binding, conformance, the gate's verdict and the code that names the wait) |

**The artifact attestation (the least-trust source).** A kernel class registers only over an artifact root the route attests; the hook is
gone from production (the list is empty outside a test) and the attested set is now *derived*: the kernel root of every artifact binding
that is Matured or Final and not refuted, replacing the ledger's attested rows every block. Why not "checked by the fold": the V2
registry's `artifact_root` is a Merkle root over `(tensor name, layer, byte offset, bytes)` leaves and the kernel's `ParamCommitmentsV1`
root is over per-tensor row/column commitments — roots of the same bytes in two unrelated hash structures. No function of the two roots
shows they agree, and the fold has no bytes, so equality **cannot be checked at registration without the bytes (RFC-0014 §16
availability)**. What the chain does instead is make the statement bonded and refutable: the registrant of an existing V2 class states the
kernel root (104), a slice of its free collateral is held, the root is attested only after a window, and anyone holding the artifact
refutes a false statement (105) with two openings of the same coordinates — the V2 inventory leaf (Merkle path to the registered root, at
the leaf the program's layout puts those bytes in) and the kernel's tensor row (path to the bound commitment) — that disagree in dtype,
shape or bytes, or an instance set that differs from the program's declaration. The refuted binder is slashed (half to the challenger, never
to the binder's own operator). The residual assumption is exactly the OPV one: someone capable can obtain the bytes within the horizon.

**Activation.** A V2 class with any onboarding row leaves `Registered` only when it is kernel-bound (106: the route holds the kernel class —
registered only after `PUBLIC_PROSECUTION_COMPLETE` — running byte-for-byte the class's program, over a live binding of this class's
artifact, under the network's challenge policy; one per class, so a plan or program cannot be substituted afterwards), its binding is
Final (past the refutation horizon), and a conformance commitment is on chain (107: every bound root of the RFC-0013 statement must be
the chain's own — genesis, ruleset, policy, artifact, program, plan, kernel descriptor — one per `(class, artifact root)`; a new artifact
is a new class and needs a new statement). A class with no onboarding row follows the legacy path unchanged.

**G-EXPIRY / G-RULESET (108).** The acceptance walk replaces the envelope by the registration it wraps, after: the fence,
`valid_from_daa ≤ daa ≤ valid_until_daa`, the fork-id fired digest at the block's DAA (and at `valid_from_daa`) being the one the signer
named (F-C4R3-01(b), §7 — it was `consensus_params_id`, which a merely scheduled fence moves), the wrapped object being a bought class
registration of the signer's own bond, and the signature over `(network, fork digest, valid_from, valid_until, signer, registration)`. The
wrapped registration then meets every rule it would meet carried bare. The SDK signs the envelope (§7: `SignedRegistrationRequestV1`,
detached signing through `misaka model onboard`).

| Case | Test | Status |
|---|---|---|
| Bound -> Pending attests nothing -> Matured attests -> the kernel class registers with NO hook -> kernel-bound -> committed -> held by the gate -> Final releases the reservation and the class leaves Registered; the read model says why at each step; replay | `g14_onboarding_a_class_is_bound_attested_registered_and_released_by_the_gate` | PASS |
| A false binding refuted by two disagreeing openings; honest bindings are unrefutable; the binder cannot refute itself; the root is never attested; the class never leaves Registered; second refutation refused | `g14_onboarding_a_false_artifact_binding_is_refuted_by_two_disagreeing_openings` | PASS |
| A binding over the wrong tensor set refuted by the instance-set proof alone | `g14_onboarding_a_binding_over_the_wrong_set_of_tensors_...` | PASS |
| Refusals leave the rows untouched: non-registrant, unknown class, second root, wrong signer / kernel class / policy, early statement, second kernel binding, statements under another plan / policy / artifact / program / kernel / genesis / ruleset, duplicate statement | `g14_onboarding_refusals_leave_the_rows_untouched` | PASS |
| Envelope: expired, not yet valid, another fork digest (another chain / a fence not fired), wrong signer, forged signature, extended expiry all dropped; the good one registers; fence off => dropped by name (F-C4R3-01(b), §7) | `g14_onboarding_a_signed_registration_envelope_...` | PASS |
| Fence: dormant on every preset, hashed when set, refused when armed, the value's identity | `rfc0015_*`, `the_signed_registration_fence_...` (consensus-core) | PASS |
| Object tags 104–108 pinned and round-tripped | `the_onboarding_object_tags_are_the_allocated_ones` | PASS |

## 6. GAPs (what is missing, exactly)

1. **The artifact binding is bonded and refutable, not proven.** See §5: equality of the two roots needs the bytes. A binding nobody can
   challenge is a declaration; the fences stay dormant until RFC-0014 §16 availability closes that (and the interim terms are measured).
2. ~~**Conformance EVIDENCE is not on chain.**~~ **Closed by OB-P0 (§7)**: the evidence is carried on chain (tag 109), the beacon is
   derived in the fold from future Panel-independent Finals, and the gate requires CONFORMANCE_PASSED (verified in the fold, unrefuted
   through a window), never the commitment alone. What stays open is listed in §7.5.
3. **Unbound classes are unaffected** (a policy decision): only a class that has begun onboarding is gated. Requiring a kernel binding of
   every new class is one more condition in `activate_due_classes`, and needs the route to be armable first.
4. **A kernel class registered over a root later refuted stays registered** (its claims continue under the kernel route); only the V2
   class cannot activate. Rewardability of kernel claims does not yet depend on the V2 class being Active (the `FinalReward` is unfunded
   anyway — GAP-3).
5. **The Final reward is unfunded.** `FinalReward` joins the coinbase queue like the accuser reward; nothing carves it from the subsidy.
6. **Pipeline claims have no Panel on the real node** (legacy mode) and the pipeline header has no wire form: `getPalwKernelClaim` returns
   an empty `recordHeader` for it. OPV pipeline classes need no Panel.
7. **Interim seats are grindable.** G14 does not rest on them: every seat colluding is the tested case.
8. **Per-object cost is O(rows).** Each kernel object rebuilds the ledger from the rows. A production fold needs a cached ledger per block.
9. **Verdict / settlement events are not stored**; `getPalwKernelClaim` serves the state they decided and replay reproduces them.
10. **The mempool does not run the acceptance gate** (as for every `0x4b` object); a chunk group's opener pays the slot rent.
11. The phase-1 reorg test runs without the strict-win fence; it passed in every run so far but is subject to the same tie-by-hash race on
    a deeper fork.
12. `getPalwOnboarding` and the other new ops are exercised over the grpc conversion and the model round trips, not through a running RPC
    service (the integration crate compiles with them; the daemon test asserts the not-found / malformed paths).

## 7. Phase 4 — conformance evidence (onboarding P0, OB-P0)

Branch `onboard/p0-conformance-carriage`. Everything still dormant behind `palw_probabilistic_constraints_v1` (refused at every real
height); presets, `PALW_T12_INT13_FENCES_V1`, the t12 `public_consensus_params_id` and schedule id unchanged (no `Params` field added).

```text
SDK-signed 108 envelope → V2 class → 104 bind → kernel class → 106 kernel-bind (the NETWORK'S post-commit policy) → 107 commit = an ATTEMPT
  → FUTURE OPV Finals of OTHER classes fold (FinalPathV1::PanelIndependent; the rows op 212 serves) → PalwWorkBeaconV1 locks → seed
  → 109 Post (registrant; may ride ObjectChunks, judged on the assembled whole)
      in fold, charged to the block's adjudication budget: bound to THIS attempt? (commitment, policy, the chain's own beacon and seed,
      the committed scope) — no ⇒ dismissed;  rebuilt EXACTLY from its outcomes (selection from the seed, counts, status, result roots)
      and a pass with the policy's bits? — no ⇒ CONFORMANCE_FAILED (counted)
  → challenge window (80 DAA): 109 Refute by any other operator, from public material ⇒ CONFORMANCE_FAILED (counted)
      LeafDecode: an opening of a selected leaf against the class's registered artifact root whose reference decode ≠ the posted outcome
      VectorTokens: a Final, unconvicted claim of the BOUND kernel class on the selected prompt (greedy) whose tokens contradict the posted ones
  → the window closes unrefuted and the beacon re-derives unchanged ⇒ CONFORMANCE_PASSED ⇒ (kernel class stands) G14_ELIGIBLE
  → the gate (binding Final, kernel class, record ≥ G14_ELIGIBLE) ⇒ V2 Active ⇒ the record says ACTIVE_REWARDABLE
  no lock by S + window ⇒ BEACON_UNAVAILABLE (counted, pending)   lock but no evidence by lock + 60 ⇒ withheld: a default (counted)
  attempts = beacon_retries + conformance_failures ≤ retry_limit + 1 = 3, then a further 107 is refused (AttemptsExhausted)
```

### 7.1 Design: optimistic with a bounded in-fold admission — why

`verify_conformance_evidence_v1` alone is cheap (O(k) hashes), but it judges bindings and accounting only: a forged result root passes it.
So design (a) — in-fold verification alone — would admit evidence whose results nobody computed. What the fold CAN recompute it
recomputes at posting (bounded by the chain's scope bound and charged to the block's adjudication budget): the seed, the selection, every
count, the status, the three result roots and the material locator from the carried outcomes. What it cannot — a forward pass on three
implementations, the artifact's bytes — an outsider proves from public material inside a window (design b): a leaf against the public
artifact, a vector's tokens against a Final of the bound kernel class. The evidence is on chain (the material is table 40's row), so
"available" is the carriage itself; evidence never posted is a default, never a pass.

### 7.2 What was built

| Item | Value |
|---|---|
| Object | tag **109** `ConformanceEvidenceV1 { v2_class, action: Post(evidence, scope, outcomes) \| Refute { evidence_id, fault }, signer, signature }` — `palw_onboarding_message_v1(network, 109, signer, borsh(class, action))`; admitted to the chunk lane (`palw_chunked_object_kind_admitted_v1`), its signature checked at the completing chunk; dropped by name below the route's fence |
| Rows | aux **39** `ConformanceAttemptRowV1` per class (the contract's `OnboardingRecordV1`, the attempt's full commitment, `committed_daa`, `challenge_epoch` = attempt ordinal, eligible / excluded profiles frozen at the commitment, the posted evidence's summary, how the last attempt ended); aux **40** the posted `ConformanceEvidencePostV1` (the material). Both via `KernelRouteRow` (160), tail `0xEC`, `kernel-route/v1` root — no new delta, tail or root |
| Beacon | `BeaconContextV1` from the attempt row; sources = the route's OPV classes registered at the commitment, minus the candidate and its bound kernel class; events = `PalwKernelRouteStateV1::beacon_events_v1()` (the Finals op 212 serves); nothing from heartbeat, BASE-0, EXEC, block hashes, DNS/BFT or validators |
| Policy | `palw_onboarding_challenge_policy_v1()` — the contract's reference policy, k 2, delay 2, window 120, D 2, 1 repetition, **2 bits** (INTERIM, a drill number, not a security value), retry_limit 2. Tag 106 now binds THIS policy's id (it bound the ledger's interim per-claim policy id, which is no `PostCommitChallengePolicyV1` and could never seed a challenge) |
| Evidence core | moved from the runtime pack to `consensus/core/src/palw_conformance_evidence_v1.rs` (scope, selection, outcomes, `assemble_evidence_v1`); the pack re-exports it, encodings unchanged (17/17 `runtime_pack_beacon` pass) |
| Gate | `onboarding_gate_v1` → `conformance_gate_v1`: Ready only at G14_ELIGIBLE / ACTIVE_REWARDABLE; codes are the contract's (`BEACON_UNAVAILABLE` waiting for randomness, `CHALLENGE_PENDING` in the window, `CONFORMANCE_FAILED`, `PUBLIC_PROSECUTION_INCOMPLETE`) |
| Read | `ConsensusApi::palw_conformance_evidence_v1`; RPC **231** `getPalwConformanceEvidence` (lifecycle, attempt, beacon state, posted evidence, gate, the raw rows of 39/40, the class's program) — rpc-core, proto (1276/1277), grpc, wrpc, service, integration arm |
| SDK | `misaka-palw-sdk/src/onboarding_chain.rs`: `SignedRegistrationRequestV1` (one builder, detached JSON, `sign` through `EnvelopeSigner` — no seed in the library), `conformance_evidence_object_v1`, `fresh_verify_from_reads_v1` (public reads + the public artifact; checks the artifact's inventory root first) |
| CLI | `misaka model onboard envelope-export \| envelope-sign \| status \| verify` (`misaka-cli/src/operator/model_onboard.rs`; file the envelope with `misaka palw submit-object`) |
| F-C4R3-01(b) | the envelope names `fork_id_v1(params, valid_from_daa).fired` and a window `[valid_from, valid_until]`; accepted only where the digest at the block's DAA (and at `valid_from`) equals it. Builds differing only in an unfired fence agree; an envelope signed before a fence crosses expires across it. The SDK/CLI compute the digest from the client's compiled params, never from an RPC answer |

### 7.3 Cases (real node: mempool → template → fold → persisted tip → `ConsensusApi`; `cargo test -p kaspa-consensus --lib g14_conformance`)

| Case | Test | Status |
|---|---|---|
| Happy path: SDK-signed envelope registers the class; commit; waiting for randomness (no seed, no evidence possible); two OPV Finals of another class fold, the beacon locks; evidence in 1 KiB chunks judged on the whole; CHALLENGE_PENDING; second evidence refused; an honest-leaf "refutation" dismissed and charged; no self-refutation; fresh verifier (public reads + public artifact, SDK and core) agrees; window closes ⇒ G14_ELIGIBLE; AVAILABILITY_REQUIRED until the binding is Final; then Active + ACTIVE_REWARDABLE; replay same roots | `g14_conformance_evidence_passes_only_after_an_unrefuted_window_and_the_class_activates` | PASS |
| Commitment only: never passes — BEACON_UNAVAILABLE at the window's end, counted, the class stays Registered with the binding Final | `g14_onboarding_a_class_is_bound_attested_registered_and_released_by_the_gate` (updated) | PASS |
| Wrong beacon / seed (context) / policy / scope / commitment (implementation set): dismissed, rows untouched; a failing run honestly reported ⇒ CONFORMANCE_FAILED counted; a new commitment (implementation changed): its own epoch and beacon, the old evidence stale and dismissed, its own evidence judged | `g14_conformance_evidence_against_another_beacon_policy_scope_or_commitment_is_refused_and_stale_evidence_is_invalid` | PASS |
| Forged leaf ⇒ REFUTED by an opening of the public artifact (the fresh verifier finds it too); forged vector tokens ⇒ REFUTED by a Final OPV claim of the bound kernel class (not before it is Final); withheld ⇒ default at lock + 60; three attempts ⇒ a fourth commitment refused, never Active | `g14_conformance_forged_evidence_is_refuted_withheld_evidence_defaults_and_attempts_are_exhausted` | PASS |
| Hostile refutations (absurd leaf count, ragged bytes, moved coordinates, no such check, unknown claim): dismissed, never a panic, each charged; five in a block: four judged, the fifth finds the budget spent; the honest evidence still passes | `g14_conformance_hostile_evidence_is_dismissed_or_failed_spends_budget_and_never_stops_the_chain` | PASS |
| Forged outcome list (an outcome the seed never selected) ⇒ CONFORMANCE_FAILED | `g14_conformance_a_forged_outcome_list_fails_the_attempt` | PASS |
| Fence off: dropped by name, the block stands, no route state | `g14_conformance_evidence_is_dropped_by_name_without_the_fence` | PASS |
| Replay; reorg to a heavier branch (tables 39/40 exactly as at the fork) and back; real restart over the same database (passes the window after it); pruned import through tail 0xEC at a block inside the window, folds to every root | `g14_conformance_rows_survive_reorg_restart_and_pruned_import` | PASS |
| F-C4R3-01(b) PoC: an envelope in flight does not split builds that differ only in a future fence | `g14_c4r3_an_envelope_in_flight_must_not_split_builds_...` (un-ignored) | PASS |
| Interim policy and fork digest (consensus-core); envelope round trip through detached signing (SDK); op 231 wire (rpc-core, grpc) | `the_interim_onboarding_policy_...`, `the_envelope_names_the_fork_id_fired_digest_...`, `the_envelope_round_trips_through_detached_signing_...`, model / grpc round trips | PASS |

### 7.4 The lifecycle as it now runs (codes are `misaka-palw-challenge`'s)

`REGISTERED_DORMANT` (the 107 record is built from the chain's own facts: program, plan, class) → `CHALLENGE_PENDING` (an open attempt:
waiting for randomness = gate code `BEACON_UNAVAILABLE`; evidence in its window = `CHALLENGE_PENDING`) → `CONFORMANCE_PASSED` →
`G14_ELIGIBLE` (the bound kernel class stands: it registered only after `PUBLIC_PROSECUTION_COMPLETE`) → `ACTIVE_REWARDABLE` when the gate
activates the V2 class (availability = the artifact binding past its refutation horizon). A failed / unavailable / withheld attempt returns
to `REGISTERED_DORMANT` with its failure code and is counted.

### 7.5 GAPs (P0's, honestly)

1. **Beacon availability on a real network.** Sources are OPV Finals only (a Panel-licensed Final exports no event), and OPV needs a
   class on the network's admission list; no network has one (the OPV fence is refused at every height). Until Panel-independent Finals
   exist at the policy's rate, every attempt ends BEACON_UNAVAILABLE — correct, and fatal to activation. Bias / last-contributor /
   withholding analysis of the accumulator remains RFC-0007 §VI.8's external gate.
2. **Vector logits / commits digests have no court.** Only a vector's greedy tokens are refutable on chain (through a Final of the bound
   kernel class, itself optimistic truth under the kernel route's own court and the OPV availability assumption); a forgery that keeps the
   tokens and lies in the digests is caught only by a fresh off-chain re-execution (the runtime pack's `verify-conformance`). The vector
   refutation also needs someone to post the prompt as a job and a producer to carry it to Final inside the 80-DAA window.
3. **The authenticated openings root is the poster's claim** (no multiproof is carried); its leaves are refutable individually against the
   public artifact, so it carries no security weight on chain.
4. **Binding equality (GAP 1) still bounds everything**: the leaf refutation authenticates against the V2 inventory root, the kernel class
   runs over the kernel root, and their equality is a bonded, refutable statement, not a proof.
5. **FinalReward funding (GAP 5)**: ACTIVE_REWARDABLE means the V2 class earns under V2's funded economics; the kernel route's
   `FinalReward` for its kernel class is still unfunded.
6. **INTERIM numbers**: 2 bits, k = 2, windows of 120 / 60 / 80 DAA — chosen so a drill crosses them; an activation replaces them with an
   approved tuple (RFC-0007 §VI.8). No refutation bounty: a refuter is paid nothing (a false refutation costs its carrier fee and the block's
   budget); forging costs the registrant an attempt, not collateral.
7. **The chunk lane is capturable (F-C4R3-03)**: large evidence rides `ObjectChunk`s, so eight junk groups can hold a Post (and a chunked
   refutation) off the chain — the lane's fix is the Lead's.
8. **A source Final convicted after the evidence is posted** ends the attempt BEACON_CHANGED at the window's close (counted as a beacon
   retry) — safe, but it charges an honest registrant an attempt.
9. **The fresh verifier trusts op 231's frozen eligible/excluded sets** (chain state under the aux root, checkable through op 211's rows)
   rather than re-deriving the route's OPV classes at the commitment's height.
