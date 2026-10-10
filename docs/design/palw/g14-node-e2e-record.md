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
| G14-R4 additions | consensus tag **113** `KernelRouteChunkV1` (the route's own chunk lane, F-C4R3-03; 112 unallocated); aux tables **41** chunk groups, **42** allocated and unused (both ride delta 160 / tail 0xEC / the root); kernel route tag **15** `SealProof` (inside tag 110, GAP-R7); kernel ledger tables **15** proof seals and **16** job escrows (GAP-5); settlement kinds **13** `AdmissionFee` (F-C4R3-05), **14–17** `ReserveJobEscrow` / `ReleaseJobEscrow` / `PayJobEscrow` / `JobFee` (GAP-5); policy fields `job_fee`, `job_escrow_ttl_daa`; then (lead's registry, 2026-10-09: G14R holds settlement kinds 14–21 and `LedgerEventV1` 21–24): kernel ledger table **17** served demand bonds; settlement kinds **18–20** `ReserveSealDeposit` / `ReleaseSealDeposit` / `ForfeitSealDeposit` (bonded seals), **21** `ForfeitDemandBond`; events **21** `ProofSealed`, **22** `JobEscrowReturned`, **23** `SealForfeited`, **24** `ServedDemandBondsBurned`; policy field `seal_deposit`, claim-row field `sealed_daa`; tag-113 target discriminant **2** `Conformance` (OPV-BOOT #1) |

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
| **G14-R4**: a self-inflicted default (the colluders' own demand, the producer silent) never erases a provable fraud: the reservation is held through `default + liability_daa`, the outsider's later proof convicts with no fee and an undiluted bounty; with no valid proof the reservation is released at the horizon | demand → default → proof | `g14_c4r3_a_self_inflicted_default_…` (Panel and OPV), `…_a_withheld_position_is_a_demand_then_a_default_never_a_conviction` | PASS |
| **G14-R4**: the route's own chunk lane — per-bond signed rooms, deposit held (V2-visible) and forfeited at TTL, TTL bounded by the target, wrong-kind / mis-signed / decided-target groups dropped; a chunked proof convicts while junk fills the legacy table and the colluders' own rooms | tag 113 | `g14_kernel_route_the_routes_own_chunk_lane_…`, `g14_c4r3_eight_junk_chunk_groups_…` | PASS |
| **G14-R4**: OPV lane capture — only pre-Final claims hold slots, a fresh producer is admitted past the total, a fee per admission | | `g14_c4r3_opv_two_bonds_must_not_be_able_to_hold_the_whole_opv_lane` | PASS |
| **G14-R4**: a lifted proof pays its earliest sealer (the producer's own bond lifts it: the sealer is paid, the colluders lose the whole reservation) | seal → reveal → lifted copy first | `g14_c4r3_gap_r7_a_lifted_proof_pays_its_earliest_sealer_not_the_copyist` | PASS |
| **G14-R4**: the Final reward is paid once out of the poster's escrow — across replay, a reorg that undoes the Final, a re-applied claim and the coinbase's redemption | | `g14_kernel_route_the_final_reward_is_paid_once_out_of_the_posters_escrow_…`, `…_a_covered_lie_is_convicted_after_final_…` | PASS |

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
5. ~~**The Final reward is unfunded.**~~ **Funded (G14-R4, user-pays escrow)**: the poster pre-funds an escrow at posting (plus a
   non-refundable fee); the job's first Final pays the producer out of it, once (§8). RFC-0015's `work_credit_per_claim` is still not
   released by anything — it must come from the same escrow before it is.
6. **Pipeline claims have no Panel on the real node** (legacy mode). OPV pipeline classes need no Panel. ~~The pipeline header has no wire
   form.~~ **Closed by K2S** (`k2-real-scale.md` §8): `recordHeader` = borsh `(PipelineHeaderV1, PipelineClassV1)`; no pipeline claim runs on
   the real-node E2E yet.
7. **Interim seats are grindable.** G14 does not rest on them: every seat colluding is the tested case.
8. ~~**Per-object cost is O(rows).** Each kernel object rebuilds the ledger from the rows.~~ **Closed by K2S** (`k2-real-scale.md` §8): a
   cached ledger per block (no decode, no program decode, no gate after the block's first load); the residual (a clone, a serialization and
   a diff per object; one rebuild per block) is recorded there.
9. **Verdict / settlement events are not stored**; `getPalwKernelClaim` serves the state they decided and replay reproduces them.
10. ~~**The mempool does not run the acceptance gate.**~~ **Closed by K2S** (`k2-real-scale.md` §8): the mempool and the template run it
    (`TxRuleError::PalwKernelRouteRefused`); chunks are judged at the completing chunk, and a chunk group's opener pays the slot rent.
11. ~~The phase-1 reorg test runs without the strict-win fence.~~ **Fixed (G14-R4, `39e4ea441`)**: the registration replay-and-reorg
    case arms strict-win (a shallow tie is GHOSTDAG's) and makes the deep reorg back a strict economic win (an attempt on A's tail).

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
9. **A VectorTokens refutation rests on a kernel Final, which is optimistic truth.** A griefer can post the selected prompt as a job,
   carry a LYING claim of it to Final unchallenged inside the window, and burn an honest attempt (it loses its OPV reservation only if
   someone convicts the claim within liability, and the attempt is not restored). Hardening (deciding at the claim's liability horizon,
   or restoring the attempt on a later conviction) is not done. The LeafDecode refutation has no such residual (authenticated bytes,
   deterministic decode).
10. **The fresh verifier trusts op 231's frozen eligible/excluded sets** (chain state under the aux root, checkable through op 211's rows)
   rather than re-deriving the route's OPV classes at the commitment's height.

## 8. G14-R4 (2026-10-08): the round-3 fixes, and the Final reward's funding (GAP-5)

Branch `g14/r4-fixes` (base `4d3baa0c5`). Per-finding detail, commits and tests: `adversarial-e2e-record.md`, "G14-R4 fixes". All of it
is dormant behind `palw_probabilistic_constraints_v1` (OPV parts behind `palw_panel_free_v1`); no t12 identity moves.

**Defaults keep the liability (F-C4R3-02).** A pre-Final default slashes the penalty — split like a slash: the demanders take
`accuser_reward_permille` of it (an OPV claim's at most `1000 − default_burn_permille`), the rest is burned — and keeps the rest of the
reservation until `default + liability_daa`. A valid proof in that horizon convicts (`Convicted`), paying the accuser its share of the
whole admitted reservation. A proof past any horizon, or against a timed-out claim (it never passed and never paid), is refused before
any court runs and charges no fee. What a self-inflicted default still buys the colluders is bounded by their demanders' share of the
penalty (49 BILI of 1,000 at the interim terms; 50 at the 500‰ share before ADR-0032's 49 %).

**The route's own chunk lane (F-C4R3-03).** A prosecution object larger than one carrier rides `KernelRouteChunkV1` (tag 113): every
chunk signed by the opener's Active bond, groups keyed `(opener, group)` in aux table 41, ≤ 2 open per bond, a deposit of 1 KAS per
declared part held from free collateral (in `reserved_of`, so V2's committed ledger and both exit gates see it; the kernel ledger is
synced net of it), returned at completion and forfeited (slashed) at TTL; TTL `min(64 DAA, the target's deadline)` where the target is
a kernel claim (its liability horizon, or the latest Final its clock allows plus the horizon) or an onboarding binding (its
`final_daa`); the assembled object must be a `FileProof` / `Respond` naming the target claim or a refutation of the target binding,
and its own signature is checked at the completing chunk. The certification lane (`ObjectChunk`) is unchanged.

**OPV admission (F-C4R3-05, two rounds).** Only pre-Final claims hold admission slots; a producer is capped; past
`max_live_claims_total` only a producer holding no pre-Final claim is admitted, up to the HARD ceiling `total + fresh_producer_slots`
(interim 32 + 16), which `OpvPolicyV1::validate` bounds by the window's reserved proof runs (`max_adjudications × prosecution_reserve‰ ×
window` = 64 × 500‰ × 50); every admission burns `admission_fee` (interim 1 BILI); and every block reserves `prosecution_reserve_permille`
of its court runs for `FileProof`s alone, so no flood of claims leaves an outsider without a run. Because a Final claim's reservation
stays locked through its liability horizon, holding the whole lane continuously costs `hard × reservation × (window + liability) /
window` of locked collateral plus `hard × fee` per window — 240,000 BILI locked and 48 BILI per 50 DAA at the interim terms — however it
is split across bonds (the Sybil test measures it on the example policy: 35,000 locked + 21 per window).

**Bonded claim seals (OPV-BOOT #2).** A claim seal reserves `seal_deposit` (interim 1 BILI) until it is revealed and forfeits it if it
expires unrevealed; the claim row keeps `sealed_daa` (the seal's DAA) for OPV-BOOT's sealed-source beacon v3. A seal whose job another
claim takes first can never be revealed (one claim per job) and is forfeited at expiry like a withheld one — **deliberately**: a refund
would let N bonds seal one job (one honest output, N claim ids) and reveal whichever id suits the beacon at no cost. The forfeit prices
that choice at one deposit per discarded seal; an honest producer reads the seals on chain before it seals, and its residual cost is one
deposit per lost race (a policy value; the lead accepted the design 2026-10-09). Pinned by k2_ledger_route
`a_seal_on_a_job_another_claim_took_is_forfeited_at_its_expiry`.

*Sizing `seal_deposit` = d: the expected honest loss.* If `n` producers seal the same job, one claim takes it and `n − 1` seals forfeit:
**`(n − 1)·d` burned per won claim**. Seen by one of `n` symmetric producers it is the same figure: it loses `(1 − 1/n)·d` per attempt
and wins once in `n` attempts, so it bears `(n − 1)·d` per claim it wins (each loser has also spent its production cost `C`). With the
claim reward `R` (the job's escrow), entering a race pays while `R/n > C + (1 − 1/n)·d`, so free entry settles near `n* ≈ (R + d) /
(C + d)` competitors and burns `(n* − 1)·d ≤ R − C` per won claim: the deposit cuts duplicated work, and at worst it burns the margin.

| competitors `n` | 1 | 2 | 3 | 5 | 6 |
|---|---|---|---|---|---|
| honest loss per won claim, `(n − 1)·d` | 0 | d | 2d | 4d | 5d |
| at the interim terms (d = 1, R = 5 BILI) | 0 | 1 BILI (20 % of R) | 2 (40 %) | 4 (80 %) | 5 (100 %: `n*` when `C ≪ R`) |

`n` counts the producers that seal before the winning reveal lands: the first REVEAL (not the first seal) takes the job, a seal is public
once mined and revealable `claim_seal_delay_daa` later, so a producer that sees a live seal of the job may still race, at that price. The
floor on `d` is the grinding price: choosing among `N` claim ids of one job costs `(N − 1)·d`, which OPV-BOOT's grinding accounting must
weigh against the `⌈log2 N⌉` bits it buys.

**The chunk lane's conformance target (OPV-BOOT #1).** `PalwKernelChunkTargetV1::Conformance { v2_class }` (discriminant 2 of tag 113's
target) carries a class's tag-109 conformance evidence in the route's own lane: the assembled object must be evidence of that class;
every action but a `Refute` (anyone's) must ride a group the class's registrant opened — fail-closed, so an action added later
(OPV-BOOT's `PostComplete`) is the registrant's by default; its signature is checked at the completing chunk exactly as the direct
object's (`processor.rs`), and it is then applied by the tag-109 arm. Its deadline is OPV-BOOT's `palw_conformance_chunk_target_v1(route,
v2_class, daa) -> Option<u64>`; until that lands the placeholder `palw_conformance_chunk_target_pending_v1` answers `None`, so every
conformance group is refused at its first chunk (no acceptance before the rule that bounds it). A-2: the new discriminant lives inside
tag 113, which is post-int-12 and rides unjudged below its fence as a whole.

**Salted claim seals (OPV-BOOT GAP-B1a; the lead's allocation 2026-10-09: inner kind 20, ledger tables 25 and 26).** A claim of a
deterministic class is a function of its public job and its producer, so `claim_seal_v1(claim id)` hides nothing: anyone who runs the
model computes it when the seal is posted, and a beacon over such seals is ground by its last contributor. Past `palw_panel_free_v1`
(the kernel reads it as the OPV policy's `activation_daa`; OPV-BOOT's sealed-source beacon v3 rides the same fence):

* the seal is `claim_seal_v2(claim id, salt) = H("misaka-palw/kernel/claim-seal/v2"; claim id ‖ salt)` with a 64-byte salt from the
  producer's CSPRNG (`SealClaim` is unchanged: it carries the digest);
* the reveal is ONE object, inner kind 20 `CommitClaimSalted { salt, commit: SaltedCommitV1 }`, so the salt is public exactly when the
  claim is. `SaltedCommitV1` is a separate, non-recursive enum carrying the commit's own fields under its own discriminant (5 single
  program, 6 pipeline, 19 a typed-root claim of RFC-0004 Part II — typed classes are OPV-only, so they always reveal salted past the
  fence, and that variant also needs `palw_typed_roots_v1`; K2S appends its segmented commit at integration), held to that commit's
  own ceiling plus 65 bytes;
* a seal accepted at or past the fence opens only salted: an unsalted reveal of it is refused (otherwise a sealer could choose, after
  seeing the honest salts, between "revealed but no beacon source" and a veto), and so is a salt that does not open it. A seal made
  before the fence keeps the historical unsalted reveal;
* the salt is kept, `claim id → salt` (table 25, `claim_beacon_salt`), never removed (claim rows are not either); `ClaimRowV1` is
  unchanged (its `sealed_daa` is the seal position, its `committed_daa` the reveal position);
* a seal accepted past the fence that expires unrevealed is KEPT as `(job, producer, sealed_daa) → { seal, forfeited_daa }` (table 26),
  so a withheld seal stays in the v3 mix and vetoes it instead of silently dropping out of it (SOUND SG-01a(i));
* **a re-seal past the fence forfeits the seal it replaces** (ECON F-ECON-1 / F-ECON-2, fix S1, 2026-10-09): the replaced seal goes
  to table 26 at its OWN `sealed_daa` with its deposit burned, and the new seal is bonded afresh. Before the fix a re-seal kept the
  one deposit and overwrote the row, so (1) one deposit, re-sealed under the TTL, stayed live forever and vetoed every attempt of a
  class, and (2) a re-seal inside a reveal window moved a mixed seal out of its seal window — a free, uncounted withdrawal chosen after
  the honest salts were public (2^a lockable outputs with `a` attacker seals). Now every seal position, once taken, is in the read
  until revealed or forfeited, and staying live costs a deposit per re-seal. (Refusing the re-seal outright was the alternative; the
  forfeit keeps the honest "seal another output" path and makes its price explicit.) Below the fence the historical one-deposit rule
  stands. k2_opv `past_the_fence_a_re_seal_forfeits_the_replaced_seal_at_its_position_so_no_seal_is_withdrawn_for_free`;
* `claim_beacon_seals_v1()` lists live, salted-revealed and forfeited seals together in `(sealed_daa, seal)` order (a re-seal counts at
  its latest seal) — OPV-BOOT maps it into `SealedSourceV3`;
* **the job's poster is kept** (C4R4 F-C4R4-08, agreed with OPV-BOOT): `job → poster` (table 18, `job_posters`, `job_poster()`),
  written at every job post past the fence (single-program, pipeline and typed jobs: wherever the escrow opens) and kept as long as
  the job row (never removed) — the escrow that also names the poster is spent at Final, and the v3 distinct-consumer rule must read
  the same poster at every later tip. `ClaimBeaconSealV1::poster` carries it;
* tables 25, 26 and 18 reach the root through ONE extension, `H("misaka-palw/kernel/ledger-beacon-seal-extension/v1"; base ‖ salts ‖
  forfeited ‖ posters)`, present only once any holds a row: below the fence all are empty and every root (both goldens, the
  int-12-era one included) is unchanged;
* `seal_ttl_admits_beacon_window_v1(policy, W)` states `2·W ≤ seal_ttl_daa` (a seal at the start of `[S, S + W)` must still be
  revealable at the end of `[S + W, S + 2W)`); the interim TTL of 100 admits OPV-BOOT's interim `W = 40` (pinned in core);
* below the fence kind 20 rides unjudged (A-2): the processor's kernel gate drops it beside the OPV registrations (A2U's table: row 20
  → `palw_panel_free_v1`) and the ledger refuses it with its state byte-identical.

*Retention of table 26, and its growth priced.* A row could be dropped once no beacon attempt can still count it (its seal window,
the reveal window, the mixed sources' Finals and the finality depth). That horizon depends on OPV-BOOT's v3 policy (`W`, the anchor
delay, `k`, `D`), which is not a ledger constant, and the rows are re-read by later re-derivations (the onboarding fold at the lock,
op 231, a fresh verifier), so the rows are **kept**, and the prune is a code item below. The growth is priced: a row exists only for a
seal that forfeited its deposit `d` (burned), at most one per `SealClaim` accepted (a re-seal replaces a live seal, it adds no row), and
a bond with free collateral `C` holds at most `C / d` live seals, so it can add at most `C / d` rows per `seal_ttl_daa + 1` DAA while
burning `C`. A row is 208 bytes (a 136-byte key, a 72-byte row): **1 GiB of table 26 burns at least ≈ 5.16 million BILI** at the interim
`d` = 1 BILI (`2^30 / 208 · d`), and a growth of `g` bytes per DAA burns `g / 208 · d` per DAA. Table 25 adds 128 bytes per committed
salted claim, beside a claim row that is kept anyway.

Tests: route `the_salted_reveal_is_kind_20_carries_its_commits_own_tag_and_opens_only_its_v2_seal`; k2_opv
`past_the_panel_free_fence_a_claim_opens_only_its_salted_seal_and_the_salt_is_kept_for_the_beacon` (a v1-sealed commit refused, a
salt that does not open the seal refused, the salted reveal commits, the salt in the root and the rows, replay),
`below_the_panel_free_fence_a_salted_reveal_is_refused_and_changes_nothing`,
`past_the_fence_a_withheld_seal_is_kept_as_forfeited_and_the_beacon_read_lists_every_seal_in_seal_order`; the golden roots
(k2_ledger_route, k2_opv) and core `the_interim_opv_policy_validates_…` assert the empty tables add no extension; node
`g14_opv_a_claim_commits_only_over_its_salted_seal_and_its_salt_is_kept`. Every OPV node test now seals and reveals salted
(`seal_and_reveal`), and the kernel harness salts every reveal past the fence (`common::chain::salted`). Final / reorg (the user's
"verified", 2026-10-09): node `g14_opv_the_salted_seal_rows_roll_back_and_reapply_identically_across_a_reorg` — ONE carrying block
writes a job's poster row (18), a salted reveal's salt (25) and a re-seal's forfeited position (26), its fee, deposit and posting fee
asserted conserved; a second node replays it; a two-block-deep heartbeat branch takes that node back to the fork's route exactly
(rows, aux, the three rows gone, collateral and money as at the fork); the original chain out-working it re-applies all three
identically; the node then follows the original chain to the claim's Final and the re-seal's own expiry (a second table-26 row), and a
fresh replay agrees. (The first version forked a hundred ticks back, before the first post; a node is not required to take a deep
heartbeat-only branch, so it kept its chain and the case failed — the fork is now as shallow as every other reorg case here.)

**Conservation (the user's ruling, 2026-10-09).** Every fee, escrow and slash is asserted collected from its payer, and the books
balance with no mint but the payouts the debits fund:
* *kernel, every ledger test*: the test consumer checks after every block that Σ collateral + Σ paid + burned + withdrawn = Σ declared
  (the `SyncBond` deltas), beside the existing "slashed = routed", "the ledger's burn = the burn instructions'" and "no Final reward
  without a spent escrow" checks;
* *node*: `kernel_money_v1` reads Σ collateral and Σ `slashed` of every card's bond, Σ kernel payouts minted by a coinbase or still
  queued, and the ledger's burn; `assert_kernel_conserved_v1` checks `Δcollateral = −Δslashed` and `Δslashed = Δpaid + Δburned` across
  a window — asserted for the OPV admission fee of a salted reveal, a seal deposit forfeited at its TTL (the C4R4 stale-expectation
  fix: collected from the sealer's real bond, burned, nothing paid), the post + reveal + re-seal block of the reorg case, an OPV
  default (penalty, demander share, fee), and R4X's typed
  claims (a commit and its conviction, a commit and its default), each with the payer's `slashed` checked to rise by exactly the fee
  plus the slash or penalty. (A window must not hold a debit outside the kernel ledger: a chunk group's forfeited deposit or a
  registration burn.)

**Served demands (K2S's DA griefing).** A served position's demand bonds stay reserved: refunded the moment the claim is convicted,
defaults or times out; burned only when its liability horizon ends with no conviction — a true demand that leads to a conviction (even
long after the grace) is never penalised. Pinned per fate — conviction (even post-Final), default, timeout (a claim the Panel never covers) and the
unconvicted horizon — by k2_ledger `a_served_demand_bond_is_refunded_on_conviction_or_default_and_burned_only_at_an_unconvicted_horizon`.

**Accuser seals (GAP-R7).** `SealProof` (kernel tag 15) commits `H(claim ‖ accuser ‖ H(proof))`; the bounty of a conviction goes to the
earliest seal of the convicting bytes at least `claim_seal_delay_daa` old, whoever files them. Self-recoup (the colluders' own first
proof) is priced, not prevented: `claim_collateral × (1 − accuser‰) > claim_reward`, and RFC-0015's required reservation is divided
by `1 − accuser‰`.

**DESIGN_GAP — the honest verifier's incentive (PRINCIPLES §6 condition 6; C4R4b's round-4 observation).** A convicting proof's bytes
are a function of the claim and its fault, never of the verifier's salt, and the producer of a lie knows its fault the moment it
commits. Its own Sybil therefore seals the proof at `commit + 0` (or earlier, over the claim id it is about to reveal), is the earliest
seal of the convicting bytes, and takes the whole accuser share of its own conviction. Deterrence still holds — the self-recoup is
priced above — but an honest verifier who finds the same lie is never first and earns nothing: the bounty pays the liar's own Sybil,
so condition 6 (an incentive for honest verifiers) is not met by the slash alone. No rule keyed on time after the commit fixes this
(the liar can always be first); the bounty must instead be bound to something the liar cannot know or cannot pre-empt.

*Sketch (not built — **superseded**: the user showed attacks on both parts, see item 14; lane ECON owns the question):*
1. **Pre-committed watcher salts.** A verifier bonds a watch commitment `c_v = H(salt_v)` (a new route object, bonded, one per bond)
   at any DAA before the claims it will watch. For a claim, its sample `S_v(claim) = PRF(salt_v, claim id)` selects the positions /
   relations of that claim it checks. A proof is **bounty-eligible** for `v` only if it opens `c_v` (reveals `salt_v`) and the
   convicting fault lies in `S_v(claim)`; a proof outside every opened sample still convicts (detection is unchanged) but earns only a
   fixed finder fee.
2. **A split, not a race.** The accuser share is split equally among every distinct bond whose pre-committed sample covers the fault
   and whose proof seal lands inside the claim's window (ties are not resolved by time). The liar knows its own Sybils' salts and can
   place the lie where they cover it, but it cannot see the honest salts, so each honest watcher covers the fault with its sampling
   probability `q` and then takes an equal share; the liar can only dilute, and every extra Sybil watcher costs a bonded commitment.
   With `h` honest watchers and `s` Sybil watchers covering, an honest share is `bounty / (s + (covering honest))`, and the liar's
   recoup falls from the whole share to `s / (s + q·h)` of it.
3. **Pricing.** The watch-commitment bond and the Sybil count enter the self-recoup relation (`claim_collateral × (1 − accuser‰ ×
   s/(s + q·h)) > claim_reward`), and the expected honest income per watched lie (`q × bounty / (s + q·h)` at the detection rate)
   must exceed an honest watcher's per-claim check cost from MEAS (`T_check` and fetch) — a policy relation the release states.

What it needs: the watch-commitment object and its table (an allocation), a per-class sampler `S_v` that every court can evaluate
from public data (the challenge crate's sampler shape fits), and an independent economics review. Until then the honest verifier's
reward rests on the escrow-funded `work_credit_per_claim` path (item 10 below) or on altruism — **listed as a DESIGN_GAP for
condition 6.**

### GAP-5: the Final reward's funding — user-pays escrow (the user's ruling, option A)

*Problem.* `SettlementKindV1::FinalReward` joined the coinbase queue as newly issued money while `PostJob` was free: a bond answering
its own jobs minted `claim_reward` per job (unbounded on the Panel route).

*Ruling.* Option A, user-pays escrow: the job's poster pre-funds an escrow of at least the claim reward; Final pays the producer out
of it; nothing is minted. Subsidy-funded rewards (options B/C) are out of scope and get their own design later. (Amounts below are
BILI, ADR-0174; `SOMPI_PER_KASPA` is the legacy name of 1 BILI.)

*Implementation (kernel `misaka-palw-kernel`, node fold; dormant).*
* `PostJob` / `PostPipelineJob` reserve `claim_reward` of the POSTER's free collateral as the job's escrow
  (`SettlementKindV1::ReserveJobEscrow`, row `job_escrows[job] = { poster, amount, posted_daa }`, ledger table 16, in the state root)
  and burn `job_fee` (`JobFee` + `Burn`; a real `slash_bond` on the node). A poster that cannot cover escrow + fee posts nothing.
  The reservation is the ledger's row, so V2's committed-collateral ledger and both exit gates hold it.
* The job's first Final debits the escrow (`PayJobEscrow`: a real `slash_bond` of the poster, burned at release) and pays exactly the
  debited amount as `FinalReward` (a payout row the coinbase queue mints). The escrow row is removed: an escrow pays once. A later
  claim of the same job (after a post-Final conviction freed it) finalizes with reward 0.
* **There is no unfunded reward path.** The ledger emits `FinalReward` only from `pay_from_job_escrow`; the node fold pays a
  `FinalReward` only up to what the `PayJobEscrow` before it in the same batch actually debited; the reference consumer's book refuses
  a `FinalReward` beyond the escrow spent (checked in every ledger test).
* An escrow no claim can still use goes back to its poster (`ReleaseJobEscrow`, receipt `JobEscrowReturned`) once
  `job_escrow_ttl_daa` has passed since posting, no live claim holds the job and no producer's seal of it is live — and in any case
  once one more `seal_ttl_daa` has passed, whatever seals are live (C4R4 F-C4R4-03, `a1473a3d9`: a squatter's re-seals no longer
  hold the escrow, and through it the poster's exit, indefinitely).
* Interim values: `claim_reward` (the escrow) 5 BILI, `job_fee` 1 BILI, `job_escrow_ttl_daa` 300 DAA (validated: `job_fee > 0`,
  TTL ≥ the seal TTL) — policy values like every other interim term.

*Invariants and their tests.*
* Money identity: within every receipt batch Σ debits (slashes, fees, spent escrows) = Σ (accuser rewards + demander shares + Final
  rewards + burns); hence Σ payouts ≤ Σ collected fees + slashes + pre-funded escrow, each source counted once —
  `k2_ledger_route::a_final_reward_is_paid_once_out_of_the_posters_escrow_and_nothing_is_ever_issued` (and the book's check in every
  ledger test).
* Self-posted job: the producer is paid its own escrow back and is down `job_fee` — never a gain (same test).
* Reorg / re-application / redemption: `g14_kernel_route_the_final_reward_is_paid_once_out_of_the_posters_escrow_across_reorg_replay
  _and_redemption` — a second node replays; a heavier branch from just before the Final undoes A's Final and re-finalizes on B (one
  reward minted-or-owed, one escrow debited); A out-works B again (one reward, redeemed by one coinbase, one debit); the same claim
  carried again is dropped; the virtual UTXO set holds exactly one reward output for the producer's payee.

*Not covered.* RFC-0015's `work_credit_per_claim` (nothing releases it; it must come from the same escrow before it does); the price
level (escrow = `claim_reward`) is a policy value.

### G14-R4, successor (2026-10-10): the user's design changes on this lane

**ADR-0032 — the reporter share is 49 %.** Every place the kernel route pays a reporter out of a slash uses ONE constant,
`PALW_KERNEL_REPORTER_SHARE_PERMILLE_V1` = **490‰** (it was 500‰): the convicting accuser's bounty (`convict`: 490‰ of the claim's
admitted reservation, never more than this conviction collected), the demanders' share of a default penalty (490‰; an OPV claim's
burn is the larger of 510‰ and `default_burn_permille`), and the onboarding challenger of a refuted artifact binding
(`PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1`, now the same constant). The rest of every slash is burned. The route is dormant as a
whole and has never been in force at another rate, so it needs no fence of its own beside INTF's `palw_reporter_share_v2` (R-core).
**Attacker accounting is net of the self-return:** a producer whose own Sybil accuses it recovers 49 % of the collected slash, so its
loss is ≥ 51 % of what was collected — never the gross slash. Three points for ECON (not changed here):
* *A default before the conviction.* The bounty's basis is the reservation as if no default had come first (C4 F-C4R3-02, so a
  self-inflicted default cannot dilute an honest accuser), and the demanders were already paid 49 % of the penalty. When all
  reporters are the producer's own, they recover `0.49·slash + 0.98·penalty` of `slash + penalty` collected — 53.9 % at the interim
  terms (1,000 reserved, 100 penalty), above ADR-0032's 49 %. Capping the bounty at 49 % of the total collected minus the demanders'
  share restores 49 % but lets a Sybil demander dilute the honest accuser again: a choice between the two invariants (ECON).
* *DA16's provider challenger* (`PALW_PROVIDER_CHALLENGER_REWARD_PERMILLE_V1`) is still 500‰ — DA16's constant, flagged here.
* *The honest verifier's incentive* (item 14 below) is unchanged by the rate.

**ADR-0176 — rewards draw on the bond's Q/B/R/F budget.** This lane does not build the engine (lane BUDGET, `palw_bond_budget_v1`).
The hook points are named in the code, so BUDGET has one place to call per obligation:
* `budget-accept` — `KernelLedgerV1::commit_claim`'s admission tail (beside `ReserveClaim`): the claim reserves Q, and R / F for the
  reward it can draw at Final (the job escrow's amount, GAP-5; for an OPV claim also `work_credit_per_claim`) and for any verifier
  bounty a conviction could pay; refused when the budget is spent, exactly as `need` refuses it on collateral;
* `budget-final` — `pay_from_job_escrow` (the ledger) and the node's payout arm in `apply_settlements` (`palw_kernel_route_fold_v1`):
  a reward is paid only within the reservation, re-checked at payout; nothing recovers before `d + W`;
* `budget-bounty` — `convict`: the bounty is a slash split, not issuance, but it counts against the claim's R reservation.
Until the engine exists, **rewards are not budgeted under ADR-0176**, and that is on the arming list (CODE 3).

**ADR-0177 — the chain does not interfere with model acquisition.** G14 on this route is **conditional on the verifier having
acquired the registered model**: a fresh outsider checks a claim with its OWN copy of the class's parameters, authenticated against
the registered commitments (`OutsiderV1 { artifact, .. }`), plus the claim's committed values. The route never asks a producer for
weights: a `FileDemand` names a claim's committed position, and a court reads the model operand from the verifier's copy. No
eligibility, Final, reward or slash of the kernel route depends on whether a model is served — there is no availability gate in the
kernel ledger or its fold, and none was added. Two things on this branch arrived by merges and belong to other lanes:
* DA16's artifact-availability gate in the onboarding fold (`palw_onboarding_fold_v1`: a binding past `palw_provider_court_v1` needs
  two live `Artifact` leases; the gate's `AVAILABILITY_REQUIRED` on a lapsed pair) is withdrawn by ADR-0177 (§3c (a)) — DA16b's to
  remove. (The gate's other `AVAILABILITY_REQUIRED` codes report the binding's maturity — refuted, missing, inside its horizon — not
  model availability; the code name is misleading.)
* R4X's snapshot-slice demand (`spec_slice_demandable`: any slice of a Retrieval class's registered snapshot is demandable on any
  typed claim, "though no claim commits it") is a court request for registered material, repeatable across claims until the snapshot
  is rebuilt — the cumulative scope ADR-0177 forbids. R4X / INTF to re-scope (a slice the claim's own committed item list names).
The effective detection probability `p` is acquisition-conditional and can be 0 (a closed model): the OPV relation's
`assumed_detection_permille` (interim 500‰) is a POLICY value that must come from MEAS's acquisition-conditional `p` per class.

**F-ECON-3** (one unrevealed seal stalls concurrent attempts) belongs to the GAP-B12 deposit derivation — ECON's; noted only.

**C4R4's three fixes, carried (the Lead's GAP-00, 2026-10-10).** Cherry-picked from `adv/c4r4` and adapted: F-C4R4-03 (`a1473a3d9`,
the escrow-return rule above); F-C4R4-05 (`2dfcee86e`, `LedgerPolicyV1::dismissal_fee_v1`: a dismissed filing that ran a court pays
`dismissed_proof_fee` per `1 / max_adjudications_per_block` share of the block's court work its class reserved, at least one fee, so
one junk filing against the heaviest class no longer holds a valid proof out for one fee a block; light classes pay one fee); F-C4R4-10
(`63da18025`, an onboarding object stops short of the runs reserved for proofs). C4R4's kernel PoC file `tests/c4r4.rs` comes whole
(byte-identical to `adv/c4r4`'s, so C4R4's merge adds nothing twice); its node PoC file `conformance/c4r4.rs` is left to C4R4's merge
(its A-2 tag-113 case needs A2U's central table). **OPVB's `b5d4ba90c` supersedes F-C4R4-10's rule for refutations** (a conformance
refutation is a proof: it may spend the reserve and pays `dismissed_proof_fee` when it proves nothing): at OPVB's merge, take OPVB's
`charge_route_budget_v1(.., may_spend_reserve)` and its hostile-evidence expectation.

**OPV-BOOT #1 (tag-113 `Conformance { v2_class }`).** OPVB implemented `palw_conformance_chunk_target_v1(route, v2_class, daa) ->
Option<u64>` and wired it into the lane on its branch (`opv/bootstrap-beacon` `b5d4ba90c`: the placeholder's body calls it). This
branch keeps the placeholder (`None`: every conformance group refused at its first chunk) so the two do not conflict at integration;
the node case that carries a Post and a Refute through tag 113 is still to be written after both are merged.

**Allocation note.** `LedgerEventV1::ServedDemandBondsBurned` is discriminant **24** (the registry's 2026-10-09 line gives G14R
21–24; the brief says 21–23): the Lead should confirm 24 in the registry. DA16's 30 / 31 sit after it.

### G14-R4 round 2 (2026-10-10): C4R4 round 4b's full-collusion breaks

PoCs from `adv/c4r4` @ `162df6b16` (kernel `tests/c4r4.rs`, node `da16/c4r4.rs`), each flipped or kept as stated:
* **F-C4R4-14 (P1) — the response lane.** A `Respond` spends no run of the block's shared adjudication budget (classifying it is
  linear in its own carried bytes, bounded by `max_response_bytes`), so no ordering of a block can push an honest producer's valid
  answer over budget before its deadline. A rejected response (oversized or not serving) costs its signer `dismissed_proof_fee`
  (slashed, burned; a signer that cannot pay is refused before classification) and changes nothing the producer needs: the demand
  keeps its deadline, and only the producer's own response records the demand's last rejection class. A valid response is free,
  whoever signs it. The PoC proves, for junk from any bond and from the griefer as producer of its own self-demanded claim: served in
  the first block, no default, nothing lost; every junk response paid its fee; the griefer was paid nothing.
* **F-C4R4-13 (P2).** The proof grace after a service holds the receipts' deadline too: a `Checking` claim served past it stays
  prosecutable (reservation held) until `served + proof_grace_daa`, then times out. Bounded: demands open only by the deadline.
* **F-C4R4-15 (P2) — one reporter pool per claim.** The bounty is `⌊share × what this conviction collected⌋` and a pre-Final
  default pays its demanders `⌊share × penalty⌋` (rounded down; the rest burned), so all reporters of one claim together take at most
  `⌊share × everything collected⌋` (49 %): a coalition holding every role loses ≥ 51 %. This replaces F-C4R3-02's "as if no default"
  basis. **Residual for ECON:** after a self-inflicted default the honest accuser's bounty falls from `share × R` to `share × (R −
  P)`; the difference went to the colluders' Sybil demanders (≤ `share × P` = 49 BILI of 490 at the interim terms). Restoring it
  needs the default's demander share deferred to the liability horizon and redirected to a later conviction (a held amount and its
  demanders as state — not built).
* **F-C4R4-16.** The interim share is 490‰ (`4d4d8423f`); pinned by core `palw_kernel_route_the_interim_reporter_share_is_adr_0032s_49_percent`.
  C4R4's own `c4r4_reporter_share_49.rs` needs INTF's `palw_reporter_share_v2` functions and passes once both are merged.
* **F-C4R4-17 (P2, ADR-0177 D2).** Hook `KernelLedgerV1::cumulative_scope_allows_v1(claim, stage, position)` runs in every
  `FileDemand` before anything is reserved; it admits everything until DA16b's central predicate lands (a pure function of the rows).
  The PoC stays an ignored FAIL.
* F-C4R4-19 (DA16's court vs ADR-0177) is carried as C4R4 wrote it: an ignored FAIL for DA16b.

### G14-R4 round 3 (2026-10-10): M\*-49 verifier pay and the held default share (readiness §3f)

Behind the new dormant fence `palw_verifier_pay_v1` (Params, Some-only hashed, `never()` collapse, refused when armed, needing the
route and OPV at or below it; fork-id probe arm; A2U row; kernel ledger table 27, `LedgerEventV1` 25 `CheckFeePaid` / 26
`DefaultShareHeld`). Kernel `misaka_palw_kernel::verifier_pay`, core `palw_verifier_pay_v1` (INTERIM terms, POLICY: `F` = 1 BILI,
`m` = 4, `B_cap` = 40 BILI).
* **M\*-49.** A job past the fence escrows `m·F` beside its reward escrow; the post-commit draw (`assign_drawn_v1`) keeps `F` per drawn
  slot and returns the rest; a slot is paid `F` on its attestation (`attest_drawn_v1`) or on the claim's conviction / producer default
  before the deadline; otherwise its fee returns. The fee is a user transfer (`PayJobEscrow` + `FinalReward`, the poster's debit
  funding the payout exactly) and does not draw on BUDGET's producer hook `bond_budget_final_reward_v1`. Of a conviction's 49 %
  bounty, the drawn sealers of the convicting bytes share up to `B_cap` first; the earliest sealer keeps the rest. A drawn slot's
  served demand bond is never burned (A-DEM).
* **O2.** A pre-Final default collects only its burned part; the demanders' share stays reserved on the producer's bond. A conviction
  inside the horizon slashes it into the one pool `⌊49 % × everything collected⌋` — ECON's measured case now pays the honest
  accuser **490** (was 441) and the Sybil demander 0; with no conviction the share is paid to the demanders at the horizon.
* **C9 (ECON round 4, §5e).** The interim fee is `F = F_min + ⌈r_w · S_pool / (q·m·N)⌉` = 0.86 + 0.50 = **1.36 BILI**
  (`check_fee_with_stake_capital_v1`; every input POLICY); core `palw_verifier_pay_c9_…` shows an honest watcher's books ≥ 0 over a
  fault-free window at any stake share, and in the red at `F = F_min`.
* **S3/S4 (ECON §4.4, F-ECON-3).** An opt-in beacon-source seal (`opt_in_beacon_source_v1`, consumer-called until an object carries
  it) reserves `d_src − d` beside the 1-BILI race seal (interim `d_src = d*` = 28.6 BILI), returned at the reveal, forfeited with the
  seal; it feeds exactly ONE attempt (a second opt-in is refused) and a class's beacon-source seals feed at most `N_max` = 2 open
  attempts. `beacon_source_seals_for_attempt_v1` lists only an attempt's opted-in seals; a race seal is no attempt's source. OPVB's
  v3 reader must switch to it at integration (CODE). Kernel `s3_s4_one_deposit_buys_at_most_one_attempts_worth_of_veto`.
* Tests: kernel `tests/verifier_pay.rs` (the measured case, the horizon payout, C4R4's F-C4R4-15 coalition ≤ 49 %, the fee escrow and
  attestation with exact conservation, pay on fate and drawn sealers first); core fence pin. **Not wired:** the draw (OPVB's v3
  `ClaimVerification` beacon and the watcher pool) and the attestation object (an inner kind needs an allocation) — the node injects
  the terms, so O2 and the fee escrow run on the node past the fence, the draw and attestation only through the ledger API.

### What still refuses arming `palw_probabilistic_constraints_v1` / `palw_panel_free_v1` (end of lane, 2026-10-10)

The validators (`validate_palw_probabilistic_constraints_v1`, `validate_palw_panel_free_v1`, `validate_palw_signed_registration_v1`)
refuse any armed height unconditionally. Nothing here is armable.

*CODE (a lane can write it).*
1. The validators themselves: replace the blanket refusal by the fence relations (OPV ≥ route height, the envelope fence, a validated
   ledger policy) — the last step.
2. A home for the ledger policy in `Params` (today `palw_kernel_route_policy_v1` is a code constant marked INTERIM), Some-only hashed.
3. **Rewards not budgeted under ADR-0176**: the GAP-5 escrow Final reward, the OPV work credit and every verifier bounty reserve
   against no Q/B/R/F budget — hooks `budget-accept` / `budget-final` / `budget-bounty` named above; engine `palw_bond_budget_v1`
   (BUDGET).
4. RFC-0015's `work_credit_per_claim` is released by nothing: it must come from the job escrow (GAP-5) and the budget (3) first.
5. Panel-licensed mode: the interim seat draw is grindable. Arm OPV-only, or wire RFC-0010's beacon-assigned Panel (no approved beacon).
6. Real-class carriage: K2S's segmented commitments (`k2/real-scale`); appended to `SaltedCommitV1` at integration.
7. Pipeline claims on the node: no Panel / receipt path, no header wire form (§6.6).
8. A cached per-block ledger (§6.8, O(rows) per object) and the DoS figure measured; the mempool acceptance gate for `0x4b` objects
   (§6.10); RPC through a running service (§6.12); verdict events stored (§6.9).
9. Node cases for constraint families 2–13 and 15 (the route E2E class is single-layer).
10. (closed, F-C4R4-14) responses ride their own lane; a rejected one pays the dismissal fee.
11. OPV-BOOT: the conformance chunk target merged (OPVB's `b5d4ba90c`) and a node case through tag 113; beacon v3 over the bonded
    salted seals (OPVB); the prune horizon of table 26 once v3's policy is a ledger constant.
12. ADR-0177 clean-up from merged lanes: DA16's artifact-availability gate out of the onboarding fold (DA16b, F-C4R4-19); R4X's
    snapshot-slice demand re-scoped; DA16b's cumulative-scope predicate behind `cumulative_scope_allows_v1` (F-C4R4-17).
13. Rule E (ADR-0178, FINX) armed at or below the release (fork choice).
14. The FileProof-over-budget gap, accepted and bounded (holding one valid proof out through window + horizon ≈ 1,600 BILI > the
    1,000 BILI reservation it would save); a block producer that orders the objects pays it only on blocks it does not mine.

*POLICY (the user / ECON decides).*
* Every interim value: claim collateral 1,000 BILI, demand bond 1, check / challenge / court / grace / liability 100 / 50 / 20 / 10 /
  200 DAA, exit delay 30, dismissed fee 0.1, **reporter share 490‰ (ADR-0032)**, default penalty 100, Final reward = escrow 5, job fee
  1, escrow TTL 300, 64 court runs a block with 500‰ reserved for proofs, court work 2^30, seal delay 1 / TTL 100 / deposit 1; the OPV
  terms (50-DAA window and 37-DAA budgets from a measured `T_challenge`, reservation 1,000, work credit 5, external gain 10,
  **assumed detection 500‰ — must be the acquisition-conditional `p` (ADR-0177)**, caps 3 / 32 + 16, default burn 10 %, admission fee
  1); the chunk lane (2 groups, TTL 64, 1 BILI a part); onboarding (reservation 100, window 40, liability 200); the admitted OPV
  classes; the one coordinated activation height.
* ADR-0176's `W`, the rho ↔ Q/B/R/F mapping and the caps (BUDGET implements, the user sets).
* ECON: the honest verifier's incentive (PRINCIPLES §6 condition 6 — a DESIGN_GAP: the liar's Sybil is always the first sealer of
  the convicting bytes); the reporter total above 49 % after a pre-Final default (above); F-ECON-3 (GAP-B12's deposit); the seal
  deposit against an honest producer's race loss; the demand bond against a position's carriage cost.

*EXTERNAL.* An independent soundness review (Freivalds / CRT / alias composition, exact court terminals); RFC-0011 §15.7's activation
tests and real-scale reports (9B-8k, 2M, Kimi K3); MEAS: per-class `T_check` / fetch / localize on real hardware, the window and
collateral formulas, and the **acquisition-conditional detection probability** per class (ADR-0177: 0 for a model nobody else holds);
RFC-0015 §13.3's panel-free collateral, accounting and monitoring-economics review and the inclusion / censorship assumption; an
independent adversarial node E2E with recovery and migration, a shadow period, a public drill, audit and soak. (RFC-0014 §16 artifact
availability is no longer a gate: withdrawn by ADR-0177.)

**Final halting (checked).** A valid `FileProof` accepted in a block is adjudicated in that same block (the court is a single exact step),
so it always settles before any Final. A demand accepted in the window disputes the claim and holds Final until it is served (then
`proof_grace_daa` more) or defaults — accept only while the window is open, each demand bounded by `court_deadline_daa`, the grace
bounded, Final ≤ window end + court deadline + grace (validated, and the OPV hard deadline reported). The only path to Final past a valid
prosecution is one that was never accepted (item 12), which post-Final liability still convicts.

## 9. OPV-BOOT / OPVB — derived OPV eligibility, the beacon's bootstrap, G14-for-rewards

Branch `opv/bootstrap-beacon`; the design, graph, grinding table and test list are `opv-beacon-bootstrap.md`. What changed on this path:

* **OPV admission is derived** (`palw_opv_bootstrap_v1::opv_eligibility_v1`, E1–E7), never read from a list: a tag-13 registration is
  admitted into the kernel ledger only for a class eligible at the block, and every OPV claim commits only while its class is eligible
  (`palw_kernel_route_fold_v1::opv_gate_v1`). `PalwPanelFreeFenceV1.admitted_classes` is gone; the fence carries `denied_classes` (a
  restriction only) and `min_effective_bits` (interim 128).
* **The beacon's sources** (107) are the eligible set at the commitment, minus the candidate under every mode (§7's "the route's OPV
  classes" before). Op 212's event bytes are an `AttributedWorkV1` (the event and its producer bond); the interim onboarding policy's
  source rule is the DISTINCT one (its id changed).
* **The bootstrap**: tag 106 also accepts the network's complete-check policy for a class the fold can check whole; tag 109's new
  `PostComplete` is judged in the fold (no seed, no beacon, no window) — §8 of the design for its cost bounds and carriage.
* **Test seam**: the §2 OPV and §7 conformance worlds predate derivation and name their classes through the processor's `cfg(test)`
  hook `kernel_route_test_opv_eligible_v1` (empty in every non-test build, pinned by `opv_test_eligibility_hook_is_test_only`). The
  §7 happy path's excluded set is now 3 (the candidate, its kernel class, that class's sibling under the other mode).
* **G14-for-rewards** (OPVB): past `palw_panel_free_v1` a V2 class activates and takes claims only through
  `palw_opv_bootstrap_v1::palw_reward_gate_v1` (the onboarding gate Ready and E1–E7 through its own binding); see the design's §12.
  The conformance worlds run on the drill floor (`min_effective_bits = 0`); `Cw::active_admitting_real` is X8R's helper.
