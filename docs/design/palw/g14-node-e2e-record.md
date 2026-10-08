# Kernel route on the real node — E2E record (G14 lane D, phase 2 and 2b)

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
| `kernel_route_test_attest_artifact_v1(root, from_daa)` | **GAP-1**: no on-chain artifact fact exists to attest from |
| a chain whose route fence activates at DAA 1 declares the OPV policy (activation DAA 1); `kernel_route_test_admit_opv_class_v1` | **GAP-2**: the `palw_panel_free_v1` fence and its admission list are not yet in `Params` |
| `extras.max_adjudications_per_block = Some(4)` | reaches the cross-object budget with five carriers instead of sixty-five |
| `params.palw_reorg_strict_economic_win = 0` in the harness config | a heartbeat branch against a heartbeat branch ties on every economic key; without the fence the V2 deep-reorg gate decides the tie by candidate hash (a race) |

## 4. GAPs (what is missing, exactly)

1. **Artifact attestation has no on-chain source.** A kernel class registers only over an artifact root the consumer attests public
   (`KernelLedgerV1::attest_artifact`); production attests nothing, so no kernel class can register on a real network. Needs a signed
   statement bound to a V2 model-registry artifact, or RFC-0014 §16 availability.
2. **OPV admission list and policy have no `Params` home** (phase 3 item 1). Until then no OPV class registers on a real network even
   with the fence in force.
3. **The Final reward is unfunded.** `FinalReward` joins the coinbase queue like the accuser reward; nothing carves it from the subsidy
   (the panel's claim path does). The mapping is exercised; the funding is not.
4. **Pipeline claims have no Panel on the real node.** Seat assignment and receipts are implemented for single-program claims only; a
   pipeline claim of the legacy mode is committed and prosecutable, never covered. (OPV pipeline classes need no Panel.) The pipeline
   header has no wire form: `getPalwKernelClaim` returns an empty `recordHeader` for it.
5. **Interim seats are grindable** (a deterministic race seeded by the claim id). G14 does not rest on them: every seat colluding is the
   tested case.
6. **Per-object cost is O(rows).** Each object rebuilds the ledger from the rows (class rows re-derive their program and bounds). A
   production fold needs a cached ledger per block; the rows are the consensus truth either way.
7. **Verdict / settlement events are not stored** (they are the fold's output). `getPalwKernelClaim` serves the state they decided
   (`state`, `convicted`, `rewarded`, reservation, liability horizon); replaying the chain reproduces them.
8. **The mempool does not run the acceptance gate** (it runs the lifecycle shape and funding), as for every `0x4b` object; a chunk
   group's opener pays the slot rent (`palw_object_chunk_group_rent_v1`, armed on the harness).
9. The phase-1 reorg test (`g14_registration_replay_on_a_second_node_and_across_a_reorg`) runs without the strict-win fence; it passed in
   every run so far but is subject to the same tie-by-hash race on a deeper fork.
