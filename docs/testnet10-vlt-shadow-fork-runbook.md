# testnet-10 VLT SHADOW fork — runbook (ADR-0024 activation step 3)

Status: **prepared 2026-08-11**, fence height `H` deliberately unset — it is chosen at release
cut against the live t10 DAA, not in this document. Steps 1–2 of the ADR-0024 runbook are done
and evidenced (five-validator devnet on the real Qwen3.5-2B profile: 78 committee replays
reproduced `R_j`, weighted finality certified epochs 34+ — commit `5e80356`; canonical
activation record — commit `cd3fb64`).

## What this fork is, and is not

One release moves **both** of these, together, for `TESTNET_DNS_PARAMS`:

| knob | from | to |
|---|---|---|
| `vlt.vlt_shadow_activation_daa_score` | `u64::MAX` | `H` (scheduled) |
| `vlt.model_cost_table` | `EMPTY` | `[palw_qwen36_metal_entry, palw_qwen35_2b_metal_entry]` |

They ship together because separately each is pointless or misleading: a fence with no
registered model runs an overlay in which every job mints zero; a table with no fence changes
nothing (`validator_service` idles the whole compute cycle below `vlt_shadow_active_at` — the
role resolves and logs `enabled:`, then originates, audits and declares nothing).

The weight fence (`vlt_activation_daa_score`) stays `u64::MAX`. **Finality is outside this
fork's blast radius** — that is the entire point of taking shadow first. What becomes real:

* the audit fee moves coinbase value (every counted verdict is paid at its certificate's
  challenge-window crossing), and
* settled compute challenges slash bonds (`ContradictoryVerification` at acceptance; the
  provable-only rule).

So it is a true hard fork — old builds reject the first block whose coinbase pays an audit fee
— with compute-overlay-sized consequences and finality-sized none.

## BLOCKERS from the 2026-08-11 audit — the fence may not be scheduled until these close

An external multi-agent audit (89 surviving findings, 8 critical) landed after this runbook was
first written. Three of its P0s are fixed (`dca5f94` bond-split weight inflation + the
capability-staging deadlock, `1a7838d` the unverified pruning-point overlay import). **Two
remain, both in the same class, and both are consensus rules that go live the moment the shadow
fence opens:**

1. **Forged slashing evidence via the mergeset.** `check_slashing_evidence_genuine`,
   `check_precommit_evidence_genuine` and `check_compute_challenge_genuine` evaluate the block's
   OWN body (`block_transactions_store`), while `dns_bond_mutations_from_acceptance` derives
   `BondMutation::Slash` from the whole ACCEPTED set. Evidence riding in a merge-blue block is
   therefore never signature-checked and still burns a bond. The three `Slash` arms in
   `bond_mutations_from_accepted_txs` say in comments that the block rule already proved the
   evidence — true only for own-body evidence.
   **Fix design (worked out 2026-08-11; the naive version corrupts the bond store).**
   H-05 closed the identical gap for unbond requests by splitting the check: the signature half
   view-free at derivation, the owner binding in `apply`/`revert` where the record is. Evidence
   cannot copy that split as-is — `StakeAttestation` carries a `validator_id`, not the key, so
   the signature half needs the bond record too.

   The tempting move is to hand `dns_bond_mutations_for_chain_block` the `ActiveBondView` the
   caller is already walking and drop non-genuine `Slash` mutations there. **That is a store
   corruption, not a fix.** The walk's view is as-of the block's PARENT while applying and
   as-of AFTER the block while reverting, and `slashing_evidence_genuine` reads
   `effective_bond_status(bond, target_daa)`. A bond slashed by evidence in block B is Active in
   the apply-direction view (mutation kept, stamp written) and Slashed in the revert-direction
   view (mutation dropped, stamp never undone) — an asymmetric filter, which is exactly the
   failure `dns_bond_mutations_from_txs`' own doc warns about.

   So the verification must run against a view that is a deterministic function of the BLOCK,
   identical in both directions — the same `selected_parent_bond_view` the block-validity rule
   already uses. Two workable shapes:
   * derive the selected-parent view per chain block inside the mutation derivation (correct,
     costs a bond-view walk per block — measure before choosing), or
   * carry the evidence on `BondMutation::Slash` and verify inside `apply_bond_stamp`, which
     holds the record itself; `revert_bond_stamp` needs no check because it only undoes a stamp
     whose DAA matches, so a refused apply is inert on revert **by construction**. This keeps
     the check where both directions provably meet, at the cost of a heavier mutation value
     (ML-DSA-87 signatures are 4627 bytes each).

   The second shape is the one to build: it is the H-05 pattern with the binding moved one field
   further, and its symmetry is structural rather than argued.
2. **Bond collateral is withdrawable through the mergeset.**
   `bond_spend_gate_mergeset_activation_daa_score` is `u64::MAX` on every preset, so only the
   own-body spend gate runs and a bond's locked output can be spent from a merge-blue block
   while the bond still reads Active. Slashing then has nothing behind it — which is the
   assumption the whole `λ·B_i` cap rests on.

Both must be closed **in the same release as the shadow fence**: the audit fee and challenge
slashing are exactly what that fence turns on, and #1 lets an attacker burn any bond the moment
it does. Until then this runbook's `H` stays unset.

Also outstanding from the audit, before the WEIGHT fork (step 4) rather than shadow: the
`ReplayProof` self-consistency gap (a refuting verdict costs no compute), committee-sortition
grinding (no consensus cap on live commitments), and the determinism class being coarser than
the runtime's actual bit-identity guarantee (M1 vs M4 under one `apple-metal-arm64` tag —
honest verifiers would refute honest executors on a mixed fleet). The last one interacts
directly with the hardware-class precondition below.

## Preconditions (all currently satisfied except the fleet audit)

1. **IBD equality** — a node that was not present must derive the same overlay. Evidenced by:
   pruning-import node joining live weighted finality with an identical §12 identity tuple
   (`cd3fb64` closed the one field that disagreed), and the from-genesis replay E2E
   (`scripts/misaka-vlt-ibd-e2e.sh`).
2. **`vlt_params_consistent()`** must hold for the edited preset — it asserts shadow ≤ weight,
   the §7 unbonding bound, and the credit-window/soak span. It is checked by `update_dns_state`
   before `Active`, but run the unit suite at release cut rather than discovering it in field.
3. **Fleet audit (open)** — the registered profiles are Apple-Silicon/Metal determinism
   classes. Compute participation needs ≥ 1 executor + 3 same-class verifiers **per class**, or
   every certificate stalls below `min_verifier_confirmations` and mints nothing (safe, but a
   shadow that measures nothing defers step 4 indefinitely). Options, in order of preference:
   a. **BUILT (2026-08-11): the portable CPU profile.** `qwen35_pins::CPU_BUILD_PROFILE` /
      `CPU_RUNTIME_CLASS` + `palw_qwen35_2b_cpu_entry()`, registered alongside the two Metal
      entries. The worker selects the backend at BUILD time (`MISAKA_PALW_CPU=1` against a
      llama.cpp built `-DGGML_METAL=OFF -DGGML_BLAS=OFF -DGGML_NATIVE=OFF`) and reports the
      matching identity from the same cfg, so a manifest hash cannot describe a binary that is
      not the one running. Measured on Apple Silicon: 1.7 s per 64-token job (faster than the
      Metal path at this size), and byte-identical across rerun, `verify` mode and five
      concurrent replicas. **This is the profile a Linux fleet runs**; it needs one calibration
      run on the actual fleet hardware before the fence is scheduled.
   b. count ≥ 4 Apple-Silicon validators into the fleet (only if the fleet is Apple);
   c. run shadow with zero compute (legal; proves only the fork mechanics).

   **Why the class split is not optional — now measured, not assumed.** Running the identical
   job under the CPU and Metal profiles produced the SAME `output_commitment` (the decoded
   tokens agree) and a DIFFERENT `gemm_trace_root` (the logits differ in the low bits). A
   cross-class verifier would therefore reproduce the answer and still refute the receipt. That
   is the honest-refutes-honest failure, demonstrated; `select_verifiers` drawing only within a
   class is what prevents it.

## Choosing `H`

Same A2 pattern as the 2026-08-10 flag day: `H` ≥ current tip + (fleet update window × safety
2×). Every validator/miner/seeder binary must be **inside the fleet before the flag** — a node
that crosses `H` on the old build forks itself on the first audit-fee coinbase. Use the batch
builds + collector watch exactly as `docs/testnet10-transition.md` records; the collector's
per-host version table is the go/no-go gate.

## The release diff (apply at cut, filling `H`)

```rust
// consensus/core/src/config/params.rs — TESTNET_DNS_PARAMS
vlt: VltParams {
    vlt_shadow_activation_daa_score: H,          // scheduled flag height
    vlt_activation_daa_score: u64::MAX,          // the vote does NOT move in this fork
    model_cost_table: ModelCostTable::palw_metal_registered(), // both pinned Metal profiles
    ..VltParams::INERT
},
```

`min_network_compute` stays at the production default through shadow: it gates **weight-fork
eligibility**, not shadow, and its real value is one of the numbers shadow exists to measure
(step 4 freezes it together with `R0`/`H_halving`/`D_settle` for the token program).

## Operator surface during the soak

Watch, per host:

```
[vlt-shadow] sink_daa=… N validator(s) with credit; Q/K epoch(s) would reach quorum; newest epoch …: W(E)=…
[validator-service] compute: committed to job … / confirmed certificate … — our replay reproduced R_j
[vlt-credit] … certificate(s) in the window, none credited: <reasons>
[vlt-identity] epoch=… <7-field tuple>
```

Exit criteria for scheduling step 4 (the weight fork): a **healthy, boring** `[vlt-shadow]`
line — W(E) above the recalibrated floor with a consistent would-be quorum — across the whole
fleet for at least one full credit window, identity tuples equal on every host including at
least one freshly-synced one, and the step-4 constants frozen from the measurements.

## Rollback

Below the weight fence the overlay cannot stall finality, so the worst credible outcome is
compute-overlay misbehaviour (bad fee flow, spurious slashing). The response is a point release
that pushes `vlt_shadow_activation_daa_score` forward (re-fencing), not a chain rollback;
bonds slashed by a bug would need the §10 operator process. Nothing in this fork can strand
the chain itself.
