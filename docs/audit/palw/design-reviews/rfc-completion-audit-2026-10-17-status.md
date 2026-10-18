# RFC completion audit 2026-10-17 — status of the six assigned items (integ/validation-r2)

Checked against the code at `ca5e75080` (the audit read older code). All six are still open in code. Each needs values or a
design choice that ADR-0181 reserves to the user; those are listed. No fence is armed; t12 identity unchanged.

| item | still present | what a real fix needs | user decision needed |
| --- | --- | --- | --- |
| RFC08 unsourced liability | `refuse_unsourced_admission_v1` at the four native admissions (`palw_bond_budget_fold_v1.rs` funded v11, text, job v39, mining v39); `Unsourced` legs for the material provider fee (`palw_monetary_material_fold_v1.rs`), drawn-verifier fees and beacon-source fees (`palw_kernel_route_fold_v1.rs`), which `settle_once_liable_v1` closes with a zero terminal marker | a producer-signed absolute liability horizon on each native job/claim record (today only `deadline_daa` / `commit_deadline`, not a liability horizon), a monetary-covenant TTL bound to the claim (the kernel-claim path already sources both: paid lifetime + `covenant_ttl_v1`), the bond hold until the horizon, and explicit hold → release / forfeit / refund transitions per leg | the horizon formula per native type (relative to deadline? court + grace?), whether the fee legs are held to the payer's horizon or the provider's, and the refund rule on default |
| RFC07 PanelLicensed event | `final_export_v1` returns `None` for `PanelLicensed` (the route does not hold the Panel's seed/epoch); staged trap reveal refused for interactive claims (`palw_paid_claim_trap_v1.rs` "a trap on an interactive (staged) paid claim is not supported") | carry the Panel assignment (seed, epoch, seats, dependency profile) into the route at licence time and export an event marked Panel-dependent (`validity_independent = false`), so the sealed collectors keep refusing it as a source | whether a Panel-licensed event may count anywhere (it cannot be a Panel-independent source by rule) — if not, the export is for audit only |
| RFC09 remote miner L2 | `misaka/palw/remote/src/miner.rs` attested mode still reads `PalwForkChoiceOpeningV1` / `verify_fork_choice_v1` and STOPs under Rule E | port the RemoteNode fetch, opening types, attestation, candidate window and comparator to the V2/V3 root reader the full node uses; 2-fork test under Rule E + Gate C | none (implementation) |
| RFC10 end-to-end with approved sources | no node test drives discovery → auto binding → replacement → Final without Panel injection | depends on the paid-source path (§6 of the validation record), itself blocked by the onboarding-window / paid-lifetime choice | the window/lifetime choice (pending) |
| RFC12 tooling | `scripts/drills/misaka-rfc12-acceptance.py` `NODE_GAPS`: reserve balance read, crash failpoints at import WAL barriers, paid-request generator from an authenticated snapshot, outpoint UTXO read; signed deposit-refund CLI | four node reads/hooks and a CLI; then the acceptance run | none for the reads; the run needs the `evm` build (disk) |
| RFC18 payout split | `name_certified_reward_v1` (`palw_security_certification_fold_v1.rs`) releases the held reward as one leg | split into legs below — conservation proof per option | the ratios (not chosen here) |

## RFC18: design options for the certified reward split (no ratio chosen)

Let `R` be the held issuance reward of a certified claim (already drawn at Final; no new draw). Every option must satisfy
**conservation**: the legs sum to exactly `R` (rounding remainder to a named leg, never minted), each leg is named once through
`settle_once_liable_v1`, and a conviction before release withdraws all unpaid legs (burned, not redistributed).

1. **Two legs, fixed share** — producer `R·(1−p)`, certifying verifiers (Gate C drawn set) `R·p` split equally; remainder to the
   producer. Simple; `p` is the only value.
2. **Three legs with buyback** — producer, verifiers, and a buyback/burn leg `R·b` (removed from supply, not paid). Conservation:
   `paid + burned = R`; issuance net of the burn is lower than the schedule by `b`.
3. **Vested producer leg** — option 1 or 2 with the producer leg released over `V` DAA (linear), unvested remainder forfeited on a
   later conviction inside the liability horizon. Conservation holds per tick; a forfeited remainder is burned.

Values to decide: `p` (Panel/verifier share), `b` (buyback), `V` (vesting), and whether the verifier leg is per drawn seat or per
signing seat.

## Progress 2026-10-18 (integ/validation-r2)

User decisions recorded since: ADR-0185 (fixed-seed beacon bootstrap, seal-commitment lock randomness, paid sources, onboarding
window 1,024 DAA) and ADR-0184 (RFC08 native liability horizon, RFC07 record-only event, RFC18 single certified leg). RFC07's
record-only event is implemented (`KernelFinalReadV1::panel_record`); RFC18 needs no change.

**Item 1 — conformance / opv_bootstrap migration to the real paid path:** see the validation record §8
(`consolidated-validation-2026-10-15.md`). Two consensus bugs past the dormant monetary fence fixed (PAV3 origin header cap, native
covenant lookup); 37 / 54 in the last full group run, more passing in targeted runs since. Two causes need a user decision:
v4/v5 opt-in (and the Panel v5 beacon) cannot reach their `final_by` with approved paid sources by construction, and the
onboarding binding's 200-DAA liability ends before a paid-source conformance can pass.

**Item 2 — RFC08 (ADR-0184), not implemented yet. Implementation plan** (read-only design pass, file:function targets):

- New dormant fence `palw_native_liability_horizon_v1`; only claims committed at or past it take the new rule.
- Horizon derived, not a new wire field: `final_floor + window_court`, where the Final floor is already a deterministic function
  of the producer-signed commit body and the ruleset at commit (funded v11: commit + `ExecutionHorizonsV11.challenge_window`;
  text v29: `NativeTextClaimV29::challenge_end`; job v39: `NativeAcceptedClaimV39::final_after`; mining v39: the Master Final DAA).
  The producer's signature covers every input of the formula — no wire or object-kind change, so nothing new has to ride
  unjudged below the fence. (If ADR-0184's "署名" must be a literal signed number, a versioned field on each of the four wires is
  the alternative.)
- Admission: replace the four `refuse_unsourced_admission_v1` calls (`palw_bond_budget_fold_v1.rs` funded v11, text v29, job v39,
  mining v39) with `admit_claim_liability_v1(.., NativeClaim { family })` past the fence, hard deadline = acceptance + the policy's
  worst-case Final-floor offset + `window_court`; material TTL `u64::MAX` (these families carry no covenant).
- A sourced leg `PalwLegLiabilityV1::NativeClaim { claim, family, .. }` (terms as the `KernelClaim` branch, horizon from the
  derivation); at Final the payee's fee is held (no `write_payout`), released by a sweep past the horizon through
  `settle_once_liable_v1`; a conviction before it refunds the requester and slashes (`slash_bond`); a default refunds the held fee to
  the requester; the native mining receipt vests at the horizon.
- Out of scope of ADR-0184 (stay `Unsourced`): the material provider fee (event-relative covenant), a drawn verifier fee outside a
  paid V2 claim (RFC07-5), the EXEC kernel fee without a reward row, the beacon source fee, mesh audit pay.
- 08-C (EXEC without `exec_v2_test_admit_class_v1`): a fixture with normal class registration, family certification, seat readiness
  (the native-Final suite's `ready_native_seats` already proves inventory readiness) and bond share; the native paid-material test
  fails today for exactly this (`Candidate under the model registry`).

**Item 3 — RFC09, not started. Map:** op 203 (`GetPalwForkChoiceOpening`) serves V1 leaves only; the shared V2/V3 reader is
`palw_verify_committed_leaf_v2` (`palw_fork_choice_rule_e_leaf_v3.rs`), with window/registry sub-openings in the v2/v3 leaf modules;
`misaka/palw/remote/src/l2.rs` stops at the Rule E fence (`LeafV1Insufficient`, l2.rs ~940) and already holds an unused
`verify_committed_opening_v2`. Needed: a V2/V3 opening read (op versioning), the remote comparator over those openings, and the
2-fork Rule E + Gate C test (none exists; `l2/tests.rs` has the V1 conflict-stop tests).

**Item 4 — RFC10, not started** (depends on the paid-source path above).

**Item 5 — RFC12, not started. Map:** the four `NODE_GAPS` (`scripts/drills/misaka-rfc12-acceptance.py` 69–78) have no node read
today (reserve balance store has no RPC; no failpoints; no paid-request generator; no by-outpoint UTXO read). A new read follows
the op registry pattern (`rpc/core/src/api/ops.rs`, message types, service handler, gRPC and wRPC routes). No deposit-refund CLI
exists (`misaka/apps/cli` has `evm deposit-lock` / `claim`); the refund spend needs the lock outpoint (by-outpoint read), the
timeout rule (`EvmDepositLockNotRefundableYet`) and an ML-DSA P2PKH signature script as in `validator-core` funding.

## Progress 2026-10-18, second pass (`8be23fcdf`)

User decisions of 10-18 implemented behind dormant fences (ADR-0185 / ADR-0184 notes): v4/v5 and Panel v5 beacons read opted-in
sources by seal-reveal (`collect_opt_in_seal_reveal_beacon_v6`, node / Panel history / SDK read the same rule); the onboarding binding
liability is derived (1,208 DAA); RFC08 08-A — the four native admissions are sourced with the derived horizon bound (signed
deadline + `window_court`), `refuse_unsourced_admission_v1` is gone. Verified: `opv_bootstrap` 20/20 (was 13/20, including v4, v5 ×2,
eligibility-lost), pruned-import, `rfc0010_production_fold` 26/26, `rfc0010_beacon_adapter` 9/9, the challenge crate.

Still open: x8 assertion rewrites (≈ 10, see validation record §8); RFC08 08-B (hold the native fee at Final to the horizon,
release / refund / slash — needs each family's terminal state machine to stay convictable after Final) and 08-C (the no-seam EXEC
fixture); RFC07 `panel_record` has no dedicated test yet (every Final read in the suites passes with it `None`); RFC09, RFC10, RFC12
not started.

## Progress 2026-10-18, third pass (`integ/validation-r2`)

**Item 1 (this lane, done as far as listed):** x8 rewrites and their results — validation record §9. RFC07 `panel_record` has a
dedicated test. Open in item 1: withheld-slice (an unattributed 5 KAS slash of the root card), native happy-Final (covenant lookup at
the anchor-slot attempt), and a re-run of the six rewritten tests.

**Items 2–5 move to the Codex lane.** State handed over:

- **RFC08 08-B (partial, committed `2c608053d`):** dormant `palw_native_liability_horizon_v1` (params / identity / fork-id / state
  mirror borsh-skipped; fingerprints unchanged); `consensus/core/src/palw_native_liability_hold_v1.rs` (aux 144 row) and
  `palw_native_liability_hold_fold_v1.rs` (hold / release sweep / refund); native job v39 only: `palw_native_job_fold_v39.rs`
  `credit` holds the fee, `credit_after_final_conviction`, `accusable_v39`, `accusation_horizon_v39`, `native_dispute_open_v39`;
  `NativeJobBookV39::convict_after_final` (`palw_native_job_v39.rs`); court / DA request / DA default accept a held Final
  (`palw_native_protected_fold_v39.rs`, `palw_native_public_fold_v39.rs`); Master waits for held components
  (`palw_native_master_fold_v39.rs`); sweep hooked in `palw_kernel_route_fold_v1.rs` closing tick. Core unit tests only. Remaining:
  text v29 (`palw_native_text_fold_v29.rs` `settle` ~1268) and funded v11 holds and post-Final accusation; mining receipt vesting;
  node E2E. ADR-0184 note added.
- **RFC08 08-C (fixture written, failing):** `Cw::no_seam`, `admit_class_without_seam` (exec_slices.rs), two tests in
  `exec_slices/native_final.rs`. The class never leaves `Candidate` in 12,000 DAA: ADR-0147's network-drawn admission jury
  (`transition_builder.rs` `admission_jury_v1` ~6487; audit period, seed anchor of the span before), then Prefetching → Probation.
  Observation: this jury is a model-level review in the legacy registry path, which PRINCIPLES §9 says is not adopted — flagged, not
  changed.
- **RFC09 (implemented, test not passing yet, `aad79f5e5`):** op 203 v2 (committed leaf, v3 window, v3 registry openings; rpc core /
  gRPC / service; `ConsensusApi::palw_fork_choice_openings_v2`); `misaka/palw/remote/src/l2_rule_e.rs` (verify with
  `palw_verify_committed_leaf_v2`, `verify_window_suffix_v3`, `palw_rule_e_verify_registry_v3`, `palw_rule_e_pair_from_openings_v3`,
  `palw_rule_e_order_v2`; STOP on missing/tampered); miner `rule_e_mode_v2` replaces the `LeafV1Insufficient` STOP; node test
  `hb_fork_choice_probe/finality_consistency/rule_e_remote_v2.rs` fails at setup (Gate C needs the kernel route fence — now armed,
  not re-run). Issuer still signs v1 attestations only.
- **RFC10:** not started (needs the paid-source Panel v5 beacon wired into `t12_permissionless_panel_e2e.rs` instead of
  `reference_source()`).
- **RFC12 (partial, `24af8f6aa`):** op 246 `getPalwAcceptanceReads` (reserve balance per block, UTXO by outpoint) + harness
  `acceptance_reads`; pruning-import crash points compiled into the private-integration build; `misaka evm refund` with
  `build_deposit_lock_refund_tx`. Remaining: paid-work request generator, reserve reconciliation rule, harness arming of the crash
  points, the `evm` build and the acceptance run (not run).

## Pre-merge Codex measurements (commit c514deee4)

The measurements below apply to c514deee4 before adopting validation 8478ec5dc. Shared implementation conflicts now use validation; these are historical results, not merged-tip acceptance.


## Codex handover 3 — 2026-10-18 (in progress)

User authorized base `042e5e759` and branch `codex/rfc08-09-10-12`, with RFC09/RFC12 first. RFC08 08-B/08-C and RFC10 remain
waiting for a newer `integ/validation-r2` commit to merge. Validation-lane changes win any conflict; the touched paths must be
identified in the merge/fix commit message. No preset, activation height, identity or frozen design is changed.

**RFC09 implementation in progress:** op 203 builds the committed leaf using the full node's V1/V2/V3 grammar and exposes bounded,
versioned window/registry sub-openings. Empty proof requests retain request-v1 bytes; below the commitment fence the RPC retains
its legacy diagnostic leaf separately from authenticated openings. The issuer checks openings through the common reader before
signing. The remote Rule E path uses that reader, selected-parent walks, authenticated window suffixes and fork-registry
membership/absence/capital, the shared Rule E accumulator/decision functions, and the finality/provenance guards. Missing or
invalid evidence refuses the Rule E path before signing.

**Verification:** tests added, not yet reported as passing: core grammar/root/point tampering and sub-opening completeness;
RPC v1 compatibility / v2 round trips; remote issuer grammar/version binding; actual-node op-203 V2/V3 reads; a two-branch node
comparison under Rule E + Gate C (the no-certified-credit case), including missing and tampered evidence. Initial targeted core
build is in progress. No RFC completion claim is made. Socket-level RPC, populated certified-work fork behavior and the miner's
signing refusal still require verification; RFC12's four capabilities, signed refund CLI and acceptance run are not implemented
by this change.

**RFC12 partial implementation (same handover, unverified):** added the non-default `rfc12-import-failpoints` build feature,
which reuses the environment-and-owned-directory armed OS-kill rendezvous previously available only in unit tests. Existing
UTXO clear/copy and import-completion checkpoints are included, together with before/after synchronous barriers for session
begin and PALW/EVM/legacy sidecars. The private profile manifest advertises whether the feature is compiled. Default node builds
retain no pause/environment path. Acceptance-driver wiring and the crash/restart run remain outstanding; this does not close
checkbox 3 or the other three node-tool gaps.

**Verified core subset (2026-10-18):** `palw_fork_choice_rule_e_leaf_v3::tests` **5/5 pass**, including the two new RFC09 tests
and the three existing V3 root/certification/compatibility tests. Command-local core test optimization is disabled; the first
execution at the default 2 MiB test-thread stack aborted with stack overflow. The unchanged test binary passes with
`RUST_MIN_STACK=67108864` (the validation lane's documented debug-stack setting). RPC/remote/node tests remain unverified at
this checkpoint; no end-to-end completion is claimed.


### Codex handover 3 continuation — 2026-10-18 (implementation, acceptance pending)

- RFC09: core leaf/opening tests 5/5 and RPC V1/V2 transport tests 2/2 passed. A check of RPC service, gRPC core, remote
  binaries (`rpc` feature) and CLI passed before the subsequent RFC12 additions; current-tip verification is still required.
  The real-node tests have not run yet: their first compile found a missing `VirtualStateStoreReader` trait import in the new
  RFC12 read, now corrected. No socket-level or populated certified-work fork result is claimed.
- RFC12: added read-only op 214 (`getRfc12State`) for block reserve balances and up to 64 exact UTXO points, retaining a single
  virtual-generation lock for all requested points and propagating store errors. A missing reserve row is a known zero only
  with retained validation data/genesis, or is read from the matching pruning snapshot; unavailable history stays unknown.
  Added `evm deposit-refund` and an owner/timeout-bound ML-DSA builder; script-engine and tamper tests are written, not yet
  reported passing. The request generator runs inside the exact private paid-v4 factory, checks node genesis/params identity
  and authenticates the complete carriage against an explicitly pinned live header before creating a V4 request. The V4
  request codec is shared with the exporter without changing its bytes.
- RFC12 acceptance driver: exact WAL rendezvous is armed after genesis initialization and before the owned import link;
  restart clears arming. Bridge checks now read original inputs and materialized outputs by outpoint, link the refund to its
  original input/body and check value conservation, and compare those rights after restart/reorg/join. Python controls:
  existing acceptance tests 52/52, new point-read controls 4/4, exact-WAL protocol controls 4/4 pass (synthetic unit controls,
  not real-process observations). Full acceptance, reserve accrual/drip reconciliation and all runtime tests remain open.
- All changes remain dormant with respect to shipped consensus presets. No RFC08/RFC10 implementation has started; the fetched
  validation branch still names `042e5e759`, so the required newer validation commit has not yet been merged.

### 2026-10-18 — RFC09/RFC12 follow-up verification

- Actual consensus harness `t12_fork_choice_commitment`: **4/4 pass** (V1 fence compatibility, V2/V3 op203
  openings/subproofs, two real no-credit forks under Rule E + Gate C + capital, exact UTXO and known-zero reserve reads).
  The two-fork fixture now copies each full node's served DNS gate facts into the issuer's attestation. Missing subproofs
  and substituted committed roots refuse. This is an API harness, not socket-RPC or populated certified-credit coverage.
- `rfc12_refund_is_timeout_bound_owned_and_validates_with_the_real_script_engine`: **1/1 pass** through the real script engine,
  with changed output/amount/timeout, wrong key, early refund and excessive-fee refusal controls. No private-node
  broadcast/reorg/restart refund run has been recorded yet.
- Private paid-v4 node with EVM and `rfc12-import-failpoints`: compile check passed. The generator additionally shares the
  exporter's producer-input allocation preflight and receives the acceptance driver's pinned `{REQUEST_BLOCK}`. Current-tip
  CLI/gRPC checks, standalone input preflight tests, V3 miner signing-refusal controls and positive reserve/drip API assertions
  are still being verified; compilation alone is not runtime acceptance.
- Python exact-WAL controls **5/5 pass**; an exact rendezvous cannot share its launch with a coarse import-phase kill.
  The earlier 52 acceptance controls and four point-read controls remain unit controls only. Full acceptance and independent
  reserve-flow reconciliation remain open. Documentation checks report zero new issues (43 existing baseline occurrences).
- A fresh fetch still names validation base `042e5e759`; RFC08 08-B/08-C and RFC10 remain gated on the required newer merge.

- Follow-up results: current private node + CLI + gRPC client compile check passed. The actual-chain
  `pos_v2_reserve_drip_pays_finalized_epoch` test passes **1/1**, checking op214's consensus read against the
  independently calculated positive slash split, its later coinbase drip, and the unchanged historical balance.
  The retirement acceptance phase now records bounded per-block reserve reads; independent process-level flow
  reconstruction remains open. Retirement reconciliation controls pass **13/13**; acceptance controls remain **52/52**.

- Remote results: `l2::tests` **17 pass, 1 ignored** (the ignored item is the existing cost benchmark), and
  `miner::tests` **17/17 pass**. The latter includes the new V3 successful-publication control and missing/tampered
  opening refusals before execution/signing. These are scripted remote-node controls, separate from the real-chain
  two-fork API test above. Relevant legacy DNS anchors are walked through authenticated selected-parent openings,
  including anchors before the Rule E fork point. A mixed pre-Rule-E fork still fails closed rather than using a
  cross-activation comparator; no activation or complete migration acceptance is claimed.

### 2026-10-18 — validation merge

Merged validation 8478ec5dc, newer than required base 042e5e759. Incoming implementation wins all conflicts. Canonical op203 extras, op246 acceptance reads, remote rule_e reader, refund command and native job liability holds are retained. Codex op214 and its duplicate DTO are superseded. Paid-request generator and bounded carriage capture remain; adapters/tests/docs must be reconciled against the merged APIs before the next build. The c514deee4 runtime build and queued RPC test were deliberately interrupted at this source transition; neither is runtime evidence.

### 2026-10-18 — adapters after the validation merge (work in progress)

- The merged remote/RPC service/CLI compile check passes (`8df6acf6e` plus adapter work). The validation op203 extras,
  op246 acceptance RPC and `evm refund` remain canonical; the withdrawn branch-only op214 is not reintroduced.
- Ported the owned exact-WAL arming and the original-input/refund-output checks onto that driver. The strict point reader
  now validates raw op246 lists before normalization. Refund wiring uses the validation owner's destination and refuses a
  plan naming another destination. Live request-block pinning is restored for the bounded first-party generator.
- Merged Python controls: existing acceptance **54/54**, exact-WAL **5/5**, point reads **4/4**. These remain synthetic
  controls. A real-script refund test has been ported to validation's `build_deposit_lock_refund_tx` and awaits execution.
- Node gaps still to fix: op246's legacy optional store calls can hide read errors and do not take all points under one virtual
  lock; missing known-zero rows need explicit retained-history handling. The older pre-merge stronger reader has not been
  silently treated as merged. No merged-tip OS crash, refund broadcast or full acceptance run is claimed.
- RFC08 08-B review is now based on validation's native job39 hold implementation. Text29, funded11, mining vesting and
  post-Final family transitions still need completion. The targeted merged hold tests are running; 08-C and RFC10 follow.

- Merged baseline results: native hold unit/codec/preset checks **7/7** and the canonical refund builder's real-script
  controls **1/1** pass. Required identity targets pass: `palw_activation_pool_is_t12_only` **5/5**,
  `palw_the_release_did_not_move` **9/9**, and `g14_close_legacy::testnet12_identity_did_not_move` **1/1**.
  Earlier zero-match unit invocations were not counted as tests; these counts are the actual integration targets.
- The issuer now reads validation's `committed_leaf` with the shared V2/V3 reader. A concrete merged miner defect was found:
  it entered the V3 reader only when peers showed more than one candidate. Dispatch now covers the ordinary single-tip
  case too, retaining the validation comparator. Its normal-publication and missing/tamper no-signing controls are being
  verified; mixed-activation and V2 non-Gate-C comparison remain open.

- Resource stop: shared free space fell to **24 GiB**. The owned guard interrupted the issuer check and the queued remote
  miner test before completion, as required by the 25 GiB rule. They are **not** counted as passing. The nine/ five/ one
  identity results above and seven hold / one refund result completed before this stop. No EVM build starts below 30 GB.
  Compiler artifacts were cleaned only inside this lane's own `/tmp/misaka-rfc0912-target`; other lanes were not stopped
  or cleaned. Remaining source work continues without a build until the free-space rule permits it.

### 2026-10-18 — RFC08 08-B text29 book transition (partial)

- Added the crate-private `NativeTextBookV29::convict_after_final` transition in
  `consensus/core/src/palw_native_text_stop_v29.rs`. It checks the original Final outcome, principal, inclusive signed
  horizon supplied by Core, independent finder and collectible-reserve bound; replaces the same unpaid principal with a
  requester refund; and scales finder/burn from the actual collection. It changes no serialized field or wire object.
- Added two focused book tests: full/short/zero collection with exactly-once principal movement and rooted restoration;
  invalid evidence marker, non-independent finder, excess collection, late conviction, short horizon and wrong phase must
  preserve the original book. These are synthetic book controls, not full native admission/court/payment evidence.
- **Not yet tested:** the targeted Rust build was interrupted by its owned guard when shared free space again fell below
  25 GiB. The command used `CARGO_INCREMENTAL=0`, `-j 3`, core opt-level 0/debug 0, and the existing owned target.
  Formatting and diff checks pass; no Rust test or completed build result is claimed for this addition.
- **Still required for text29:** connect the verified held row and this transition to the real court/DA-default adapters;
  hold the fee at Final, keep collectible reservation and accusation/source rights until the horizon, and close them on
  release/refund. Funded11, mining vesting, job39 end-to-end evidence, 08-C and RFC10 remain open. The helper alone does
  not complete 08-B and is not reachable from a new public wire path.

- Follow-up in progress: added `native_fee_needs_hold_v1` and connected job39's Final credit to it. A valid Final after the
  already-fixed signed horizon should take the mature payment path rather than constructing a row rejected for H < Final;
  H is not recomputed from the actual Final. Added below-fence / H-1 / H / H+1 controls. This change is not yet counted tested.
- Free space later recovered to 34 GiB. A single owned verification chain is running with a fresh 25 GiB stop guard:
  `/tmp/misaka-rfc0912-run-config/verify-core-and-remote.py`. It builds the core test executable, runs only text-book and
  native-hold controls, then checks the canonical issuer and runs the single-tip V3 miner control. Results are pending;
  logs use `/tmp/misaka-rfc08-core-no-run.log`, `rfc08-text-book-current`, `rfc08-hold-current`, `rfc09-issuer-check-current`
  and `rfc09-miner-single-current` log names. This is not the EVM/runtime acceptance run.

- RFC12 refund CLI follow-up: require exactly the requested canonical outpoint and no unsolicited reserve rows in op246; use the response virtual DAA for timeout validation and JSON output. This closes the previous first-row assumption and separate-connection-DAA mismatch. Compilation/runtime verification of this CLI follow-up is pending; the node atomic-read work and live acceptance remain open.

- Verification result (2026-10-18): core test executable compiled successfully with incremental disabled, core opt-level 0/debug 0 and `-j 3`. `palw_native_text_stop_v29::tests`: **4 passed** (including held-Final conviction and invalid-conviction immutability); `native_liability_hold`: **8 passed** (including the new fixed-horizon maturity control). Logs: `/tmp/misaka-rfc08-text-book-current.log`, `/tmp/misaka-rfc08-hold-current.log`; command/evidence summary: `/tmp/misaka-rfc08-core-book-hold-summary.json`. These are bounded book/hold controls; full 08-B, 08-C and RFC10 remain unverified. The later CLI change was not part of these tests. Documentation check: no new issues, 43 existing baseline occurrences.

- RFC12 atomic reader work in progress (2026-10-18): op246 now delegates to a bounded consensus batch method holding pruning protection and one virtual state/UTXO lock. It propagates storage errors and emits canonical `txid:index` points. Missing reserve rows map to known zero only with retained acceptance/genesis evidence or a matching pruning snapshot; unknown history stays unknown. Added real-pipeline zero/unknown and exact-UTXO/stamp/bounds controls, not yet executed. The earlier issuer check started before this API addition; it cannot establish the new reader compiled. Live acceptance and crash/reorg evidence remain outstanding.

- Canonical remote issuer follow-up compilation passed (`cargo check -p misaka-palw-remote --features rpc --bin palw-fork-choice-issuer -j 3`, `/tmp/misaka-rfc09-issuer-check-current.log`). It began before the new atomic API edit and is evidence for the issuer adapter only. Miner control, CLI check, new consensus batch tests and RPC service check are queued/running in the owned target; none is yet counted passing.

- Refund CLI follow-up compilation passed (2026-10-18): `cargo check -p misaka-cli -j 3`, incremental disabled, core opt-level 0/debug 0, owned target; `/tmp/misaka-rfc12-cli-check-current.log` finished successfully. This covers the exact-point/count/no-unsolicited-reserve checks and using the op246 response DAA in refund construction/output. It is not an actual broadcast/refund/reorg run; the atomic node implementation and its real-pipeline tests remain pending.

- RFC12 accepted-flow evidence work (2026-10-18): the first-party retirement scan now requests exact accepted transaction IDs, checks page coverage/duplicates, resolves each accepted ID against the accepting block/mergeset/selected-parent bodies, and retains only accepted slashing/precommit evidence payloads for reserve inflow reconstruction. Unaccepted evidence in a body is not counted. Missing acceptance/body data refuses observation. The standalone historical classifier keeps its existing scan mode. Synthetic controls: retirement reconciliation **14/14**, acceptance controls **54/54**, retirement drill **5/5**. Summary: `/tmp/misaka-rfc12-accepted-trace-test-summary.json`. This supplies raw accepted evidence; positive accrual/drip reconstruction, the reserve judgement and actual acceptance run remain incomplete. No full reserve reconciliation or runtime acceptance is claimed.

- RFC09 V3 miner control now passed: `rfc09_v3_miner_never_signs_missing_or_tampered_openings` **1/1**, remote `--features rpc --lib`, owned target, incremental disabled, `-j 3` (`/tmp/misaka-rfc09-miner-single-current.log`). The single-tip fixture exercises successful publication plus missing/tampered openings stopping before execution/signing/submission. It is not the real populated two-fork full-node comparison and does not establish V2 fork support. The owned verification chain finished successfully; the separate new op246 consensus tests/RPC check are still running.

- Atomic reader test result / follow-up (2026-10-18): the new concrete consensus reader compiled; zero-vs-unknown passed, while the exact-UTXO control failed because its chosen first coinbase point was absent (**1 passed / 1 failed**, `/tmp/misaka-rfc12-atomic-consensus-tests.log`). The fixture now mines an initial reward-bearing parent before choosing a coinbase and explicitly requires a nonempty payment. This correction is pending re-test; the failed run is retained and is not counted as acceptance.
- RFC09 V2 support work in progress: op203 now selects V2/V3 window and registry grammars from the committed point; the remote uses its own Params for attestation form, the corresponding full-node window/registry verifier and pair accumulator, and stops conflicts spanning activation forms. V2 membership is never treated as V3 capital. Because V2 does not commit the appended certification field, the remote derives NotGoverned from its inactive Gate C rule instead of trusting that field. The CLI parser refuses partial/misaligned registry decoding. Added V2 controls and extended the actual two-fork fixture to both forms, with nonempty windows and missing-registry refusal. These changes and the corrected atomic reader are under one owned verification chain (`/tmp/misaka-rfc0912-run-config/verify-v2-and-atomic.py`); no pass is yet claimed. V3 single-tip 1/1 from the prior source remains historical evidence.

- V2 follow-up controls: the first registry control incorrectly expected a singleton membership opening to be invalid for every other key; the shared verifier correctly also uses it as an edge non-membership opening. Corrected that assertion to require `false`, and added actual substituted-key/root tampering refusal. The failed log is retained at `/tmp/misaka-rfc09-v2-reader-controls-first-failure.log`. Current V2 reader controls **2/2 passed**, and the V3 single-tip missing/tampered-opening miner control **1/1 passed again** after the V2 changes. Binary check, corrected atomic-reader tests, real V2/V3 fork controls and positive reserve control remain in progress; summary `/tmp/misaka-rfc0912-v2-and-atomic-summary.json` records each completed step without full-scope acceptance.

- Positive reserve control now passed on the previously built concrete consensus test executable: `pos_v2_reserve_drip_pays_finalized_epoch` **1/1**, `/tmp/misaka-rfc12-positive-reserve-merged-binary.log` (5.61 s), artifact identity in `/tmp/misaka-rfc12-positive-reserve-merged-binary-summary.json`. The node accepted the real slashing flow, read its positive reserve balance against the independently computed slash split, then paid the epoch drip; the canonical reader also preserves known zero after draining. This older executable is not evidence for the later V2/V3 comparator edits or the corrected coinbase fixture. Independent acceptance-script flow reconciliation remains incomplete.

- Corrected atomic-reader controls now **2/2 passed** (`/tmp/misaka-rfc12-atomic-current-controls.log`): a real positive coinbase output is read exactly at the common virtual stamp, absent/duplicate points retain their identity and both 64-entry bounds refuse overflow; zero/unknown history remains distinct. The initial failed fixture selected an absent first coinbase point; the current fixture chooses a positive payment after a reward-bearing parent.
- Real fork result: V2 remote/full-node comparison **1 passed** with nonempty claim windows and missing registry/window/leaf/attestation plus tampering controls. The Gate C V3 case **failed** at full-node agreement: its uncertified claims have zero economic credit and deep-tie reorg protection keeps different incumbents. The verifier never established a winner. No parameter/gate was weakened to make this pass. A normally certified positive claim fixture is still required. Full result **1 passed / 1 failed**, preserved `/tmp/misaka-rfc09-rule-e-pair-first-v3-failure.log`; the sequential chain stopped, so its later RPC/positive-reserve steps did not run. RPC service check was started separately.
- Retirement evidence source correction: the first-party scan now uses the plan-declared archival role (already launched with `--archival`) and records that role; it waits for both archive and JOIN to cross the observation boundary. JOIN may legitimately prune the old bodies before this phase. Existing acceptance controls **54/54 passed** after this change; the actual archival scan has not yet run. Independent reserve-flow arithmetic and live acceptance remain incomplete.

- Atomic-reader hardening completed its scoped controls (2026-10-18): a candidate commit can stage acceptance/reserve cache entries before publishing its durable UTXO-valid status. The checked reader now acquires that status guard first and never treats such staged zero/positive rows as committed history. Real cached-store staging plus the existing exact-point/stamp/bounds and zero/unknown controls: **3/3 passed** (`/tmp/misaka-rfc12-atomic-staging-controls.log`). The positive slash/reserve/drip node control passed again **1/1** (`/tmp/misaka-rfc12-positive-reserve-after-staging.log`, 4.46 s). RPC service compilation passed (`/tmp/misaka-rfc12-atomic-rpc-current-check.log`). Exact source hashes and result counts: `/tmp/misaka-rfc12-atomic-staging-summary.json`. This establishes these bounded consensus reads; a live op246/refund/crash/reorg acceptance run and independent reserve-flow reconciliation are still required.

- V2/V3 transport/comparator milestone is retained with its limits: op203 serves the leaf-selected window/registry grammar, remote V2 membership and V3 capital stay separate, own-Params attestation checking prevents a form downgrade, and V2 derives its uncommitted certification field from the inactive Gate C rule. Source comments now describe both grammars (comment-only changes after the scoped checks). V2 reader controls 2/2, V3 single-tip regression 1/1 and miner binary compilation passed; the actual V2 two-fork case passed, while the V3 positive Gate C fixture remains red because no claim has completed normal certification. That failure is preserved rather than disabling Gate C, changing thresholds, or relabelling an uncertified fixture as positive evidence. Requests spanning more than 64 registry bonds also still need chunking. This milestone does not complete RFC09.

- Gate C positive-fork fixture investigation (2026-10-18): `T12Chain::build_attempt_drawn` explicitly uses synthetic trace/output/execution roots (its own module doc states this). Those claims cannot receive an honest positive replay attestation. Added a separate real-attempt helper: registered Base0 artifact, actual anchor-derived job for every class draw, unchanged class lottery, shared producer/remote assembly, signing only a winning execution, and ordinary full-node block admission. Its controller derives the job from the accepted header, checks material against stored claim roots and re-executes before returning a transcript. A truncated-material refusal control is pending compilation/run (`/tmp/misaka-rfc09-real-attempt-first.log`). This does not yet create any Gate C certificate.
- A second real-fixture constraint remains: testnet-12's unchanged seven-day certification prosecution span is 5,040 DAA, whereas op203 currently refuses historical state rebuilds more than 4,000 DAA behind the sink (`PALW_STATE_PROOF_MAX_LAG_DAA`). A positive fork rooted before its claim therefore needs authenticated openings retained while fresh or an appropriately bounded historical serving path; simply waiting and fetching every old opening at the end will not work. Neither certification timings nor the rebuild cap were changed. This is recorded as remaining integration work, not waived evidence.

- First real-attempt runtime control failed its declared **120 s actual-execution budget**, before a winning claim was admitted (**0 passed / 1 failed**, 121.10 s). The failed log is preserved at `/tmp/misaka-rfc09-real-attempt-debug-budget-failure.log`. Added public class-target/execution-count timing diagnostics and restarted only after that terminal failure, with the same roots/job derivation/lottery/admission checks and the same 120 s bound. The Base0 backend alone now compiles at opt-level 2/debug 0 (core/consensus remain opt-level 0/debug 0, incremental off, `-j 3`, owned target, 25 GiB stop guard); `/tmp/misaka-rfc09-real-attempt-model-opt2.log` is pending. Compiler optimization is a measurement change, not a new class target, certification parameter or test bypass. No honest certification or positive-fork success is claimed from this attempt.

- Real-execution control completed (2026-10-18): `real_floor_attempt_is_admitted_and_replayed_before_any_positive_attestation` **1/1 passed**, 68.48 s, with 247 actual model executions before admission (`/tmp/misaka-rfc09-real-attempt-budget900.log`). It independently replays the accepted header-derived job and refuses truncated material. The test now uses a deterministic reward-bearing heartbeat input and a 900 s execution budget; the class target, admission rules and certification timing are unchanged. Both previous 120 s budget failures remain recorded. This establishes actual execution/replay, not a Gate C certificate or completion of RFC09.
- RFC09 socket integration is in progress: op203 request/response conversion is shared by the production service and existing real gRPC/wRPC socket harness; the remote wire decoder is shared by the production miner and two-fork fixture. The new converter controls and socket fork test remain pending; the earlier direct-ConsensusApi fork result does not establish transport coverage.

- Shared op203 conversion controls **2/2 passed** (2026-10-18, `/tmp/misaka-rfc09-shared-converter-tests.log`): exact bond identity/window, bounded request counts and malformed bytes, and unavailable history staying unavailable. Documentation checker: 0 new / 43 baseline occurrences. Real socket fork coverage is still compiling. The fixture now distinguishes the fully armed RFC18 capital-v2 + Gate C zero-credit GHOSTDAG fallback from a Gate-C-only deep tie (expected NoRobustWinner); this follows existing §6.4 and does not certify the synthetic claims. Added registry request chunking and V2 proof attachment in the miner, pending verification.

- Real socket V2 fork control **1/1 passed**, 77.08 s (2026-10-18, `/tmp/misaka-rfc09-real-socket-v2.log`): actual gRPC adaptor → wRPC Borsh proxy → shipping client, shared production op203 converter and remote wire decoder, nonempty windows, agreement with both full nodes, missing/tampered evidence refusals. The harness supplies RpcApi handlers against real consensus, rather than instantiating RpcCoreService's unrelated P2P/index managers. Later registry chunk/attachment edits and all V3 cases are being checked separately; this result alone does not complete RFC09.

- Remote wire-reader controls **3/3 passed** (2026-10-18, `/tmp/misaka-rfc09-wire-decoder-current.log`): V2 certification normalization, authenticated registry membership, exact bond/proof association and refusal of missing/extra/truncated proof bytes or unknown leaf grammar. This build includes the miner's bounded registry aggregation code. The actual two-fork full-node/socket suite and shipping miner binary check are pending separately.

- RFC09 socket fork verification completed (2026-10-18): **3/3 passed**, 211.38 s (`/tmp/misaka-rfc09-real-socket-all-current.log`). V2 two-fork agreement, fully armed Rule E capital-v2 + Gate C two-fork agreement, and Gate-C-only zero-credit split refusing a remote winner all use actual gRPC/wRPC op203 reads and the production decoder. The fully armed V3 case exercises existing RFC18 §6.4's zero-certified-credit GHOSTDAG fallback; it does not manufacture a Gate C certificate. The earlier Gate-C-only agreement failure is retained and is now an explicit fail-closed negative. The completion audit's RFC09 transport/comparator criterion is covered by these cases; a positive certified-credit scenario is stronger additional coverage, still absent, rather than evidence established here.
- Shipping remote miner binary check passed (`/tmp/misaka-rfc09-shared-decoder-miner-check.log`); current single-tip publication/missing-or-tampered-no-sign regression **1/1 passed**, 0.11 s (`/tmp/misaka-rfc09-miner-current-after-chunks.log`). Registry fetching is now chunked at the unchanged 64-bond RPC bound, and both V2 membership and V3 capital proofs attach only to identical block/leaf/inner-root entries. No >64-bond end-to-end fixture is claimed. Summary: `/tmp/misaka-rfc09-socket-verification-summary.json`.
- RFC12 runtime preparation: started the current private-node/exporter build with `evm,private-integration-config`, incremental disabled, `-j 3`, owned target (`/tmp/misaka-rfc12-runtime-current-build.log`); free space approximately 90 GiB at launch. This is pending build/observation, not a live acceptance result. RFC12 independent reserve-flow reconciliation and actual crash/refund/reorg acceptance, RFC08 08-B/08-C and RFC10 remain open.

- RFC12 accepted outpoint lineage (2026-10-18): the archival acceptance scan now also retains every accepted transaction's exact input outpoints and output values/scripts, in acceptance order, rather than only evidence payloads. It refuses omitted input/output lists and inconsistent bodies under one transaction id; unaccepted evidence remains excluded. This supplies spent-principal/reporter lineage for independent reserve reconstruction without retaining unrelated payloads/signature bytes in the point trace. Reconciliation controls **14/14**, acceptance controls **54/54** (`/tmp/misaka-rfc12-accepted-lineage-controls.log`, `/tmp/misaka-rfc12-accepted-lineage-acceptance-controls.log`). Reserve-flow arithmetic remains incomplete and the judgement still refuses a balance-only observation; no live acceptance is claimed. Current private EVM node/exporter and CLI builds remain live in the owned target, free space approximately 88 GiB.

- RFC12 runtime preparation advanced (2026-10-18): current private node/exporter build passed, 11m14s (`/tmp/misaka-rfc12-runtime-current-build.log`). An actual offline paid-v4 factory invocation and the first-party manifest validator passed; public genesis/config digest and binary SHA-256 are recorded in `/tmp/misaka-rfc12-current-bootstrap-factory-summary.json`. Scope is offline construction only. A bounded passive single-node op246 probe is running (`/tmp/misaka-rfc12-live-point-probe.log`); CLI compilation and full bootstrap/acceptance remain pending. No positive reserve accrual, refund, WAL crash or full acceptance is claimed.
- RFC08 release sweep hardening in progress (2026-10-18): open disputes no longer occupy the first 256 release slots and starve later payable rows; native39 missing route/header/status or an unsupported family is now treated as unable to authorize payment. Added a 256-open-dispute starvation case, a payable backlog preserving the unchanged 256-release bound and an unreadable-job no-payment control. These changes are pending the focused core test build (`/tmp/misaka-rfc08-release-sweep-hardening.log`), not counted passing. This does not wire the remaining native families or complete 08-B/08-C. The already-built RFC12 node predates this dormant-fence sweep edit.

- First live op246 probe did not reach RPC readiness within its 120 s startup budget; the owned child was still computing, and cleanup confirmed it exited. **No runtime op246 pass** is claimed. Preserved failed result/log: `/tmp/misaka-rfc12-live-point-probe-budget120-failure.json`, `/tmp/misaka-rfc12-live-point-probe-budget120-failure.log`. A new same-profile/same-binary probe uses the previously declared 900 s workflow bound and a fresh owned appdir (`/tmp/misaka-rfc12-live-point-probe-budget900.log`); no consensus timing, class target, admission or RPC correctness condition changes. Its result remains pending.

- Current private-runtime CLI build passed (2026-10-18, 15m05s, `/tmp/misaka-rfc12-runtime-cli-current-build.log`). A sample of the owned retry node identified its startup work as `register_builtin_certified_families_v1` → Qwen36 actual injected-fault execution → canonical step coordinate calculation (`/tmp/misaka-rfc12-startup-budget900.sample.txt`); the process remains live. This is backend/family adjudicability coverage, not a model-tier review or claim certificate. The 900 s retry has not yet produced a runtime read result. No startup drill, readiness gate or credit requirement was bypassed.

- Real passive-node op246 observation passed (2026-10-18, `/tmp/misaka-rfc12-live-point-probe-budget900-result.json`): actual private binary + shipping RPC service + JSON WebSocket client; exact factory genesis/config identity, one present 10,000,000,000-sompi genesis fee outpoint with its script, absent outpoint, duplicate identity preserved, known-zero genesis reserve distinct from unknown history, and both 65-entry overflows refused. Owned child cleaned up after observation. Four successful response probes: maximum 22.269 ms; 200 RSS samples, sampled peak 92,880 KiB; logical DB 238,481 bytes; declared construction resource limits met. This is a single passive genesis-node observation, not positive reserve-flow reconciliation, concurrent-state/reorg evidence, refund, crash or full acceptance.
- CLI runtime regression found (2026-10-18): ordinary `misaka evm refund --help` aborted with stack overflow (exit 134, `/tmp/misaka-rfc12-refund-runtime-help.log`). Raising only that diagnostic child's main stack to the host's 65,520 KiB maximum made the same binary/help command succeed (`/tmp/misaka-rfc12-refund-help-stack64-investigation2.log`); the preceding 65,532 KiB diagnostic exceeded the host hard limit and its failure is retained. The normal entry point now starts its current-thread async dispatcher on its own bounded 64 MiB stack, with a normal error if thread creation fails. Rebuild and ordinary-environment help/command verification are pending (`/tmp/misaka-rfc12-cli-stack-fix-build.log`); no signed-refund runtime pass is claimed.

- RFC08 release-sweep hardening controls **11/11 passed** (2026-10-18, `/tmp/misaka-rfc08-release-sweep-hardening.log`, test runtime 0.01 s, build 13m11s). The new controls cover 256 open disputes ahead of one payable fee, a payable backlog still emitting at most 256 payments per block and no duplicate payment on repeated sweeps, and an unreadable native39 job retaining its fee. Existing fixed signed-horizon, refund, once-only and dormant-fence controls remain passing. The ordered sweep still scans eligible retained rows; the 256 constant bounds payments rather than every row inspection. This scoped correction does not complete per-family post-Final court/DA/collateral/retention integration or 08-C. CLI stack-entry-point verification remains pending.

- CLI startup correction verified (2026-10-18): build passed in 2m41s (`/tmp/misaka-rfc12-cli-stack-fix-build.log`); ordinary inherited main-stack limit **8,372,224 bytes**, no ulimit override, actual binary help/version/missing-required-outpoint controls **3/3 passed** (`/tmp/misaka-rfc12-cli-stack-fixed-controls.json`). The dispatcher keeps the existing current-thread async runtime on an explicitly bounded 64 MiB worker stack. This confirms startup/parser behavior only, not a signed refund.
- Started the existing first-party three-process paid-v4 bootstrap with actual registration/binding, complete code-fixture conformance, native provider/session and cold reopen (`/tmp/misaka-rfc12-current-full-bootstrap.log`, result `/tmp/misaka-rfc12-current-full-bootstrap-result.json`). Predeclared wall bound 3,600 s reflects the observed approximately 200 s passive-node startup and sequential/cold node launches; factory remains capped at 120 s. Existing 4 GiB sampled owned RSS, 2 GiB per-role logical DB and 15 s per-probe limits stay fixed; fee/binding/check caps are each 10,000,000,000 sompi. The run is pending and remains a prerequisite, not the RFC12 100k/security acceptance run. No consensus windows, targets, quorum or public preset changed.

- RFC08 native39 collateral retention work in progress (2026-10-18): the producer's bounded job catalogue is also the legacy collateral-reservation reader's index. A held Final now retains its producer slot and signed original reserve until actual fee release/refund; either transition removes only that job from the index, preserving other jobs and immutable audit rows. Collection refuses a job with an unpaid held fee, including an awaited verdict after H. This closes an accounting gap separate from the budget engine's existing withdrawal hold; no accusation horizon changes. Added rooted-header/held-row reader and release/refund index controls; compilation/run pending (`/tmp/misaka-rfc08-held-collateral-core.log`). These are accounting component controls, not ordinary admission or full 08-B/08-C evidence.
- First-party three-node bootstrap progress (2026-10-18): all three private nodes reached RPC readiness; real CLI carrier preview and actual signed submission of registration + artifact binding succeeded, **2 transactions**, total fee **654,462 sompi**, inside the fixed 10,000,000,000-sompi cap. Evidence under `/var/folders/p_/72yk2mzx0c59xvl8kpvpqg980000gn/T/misaka-rfc12-bootstrap-gmyi46di/` (`CARRIER_PREVIEW.stdout`, `CARRIER_SUBMIT.stdout`). Chain inclusion/agreement, complete-check conformance, native session and cold persistence are still pending; submission is not counted as those outcomes. Controller session remains live and is not restarted.

- Native39 held-collateral controls passed (2026-10-18): common hold/release/refund suite **12/12** (`/tmp/misaka-rfc08-held-collateral-core.log`, 0.02 s); rooted-header/held-row reserve reader **1/1** (`/tmp/misaka-rfc08-held-reserve-reader.log`, 0.08 s). A held Final remains in the producer's bounded collateral index, wrong producer rows do not join, actual fee release/refund removes only the settled job. The held-job collection guard compiled; a full collection-path control has not run. These are accounting/read-transition controls; full original admission/court/default integration remains outstanding.
- First-party bootstrap failure localized (2026-10-18): the producer generated its first PoW block `ecd1acd20d1988…`, but **A, B and JOIN all disqualified it** with `paid consumer requires already authorized material through its immutable contract`; independent RPC reads remained at genesis DAA 0. Producer retention still uses only the legacy window sum, while the active finite-consumer policy requires longer material retention. The canonical retention pin and producer facts must agree on that existing requirement. The owned controller was interrupted after this replicated invariant failure (exit 130), its finally block preserved `/tmp/misaka-rfc12-current-full-bootstrap-result.json` and cleaned up all children; evidence/cause is separately retained in `/tmp/misaka-rfc12-bootstrap-consumer-retention-failure.json`. No registration inclusion, conformance, native-session, cold-reopen or full acceptance pass is claimed. This was not a restart caused merely by an observation timeout.

- Finite-consumer producer retention correction is in progress (2026-10-18): a new candidate-DAA reader takes the maximum of the legacy window sum and the already active immutable consumer service + dispute + material spans. Producer facts, canonical attempt retention pins, chain-derived FP/gen/evaluation retention and the test template encoder use the same reader; before activation the legacy obligation remains exact. The private paid-v4 policy remains 1,000 + 3,000 + 2,000 DAA; no policy interval, lottery target, quorum, authorization predicate, public preset, wire kind or model-tier rule changes. Added producer-facts/header-pin agreement across activation and a real contract-planner control that rejects the old short promise and accepts the authorized full promise. Focused tests and the current node/exporter/CLI rebuild are pending (`/tmp/misaka-rfc12-consumer-retention-controls.log`, `/tmp/misaka-rfc12-consumer-retention-runtime-build.log`). The failed bootstrap is terminal and cleaned up; no retry has started yet.

- Finite-consumer retention controls **2/2 passed** (2026-10-18, `/tmp/misaka-rfc12-consumer-retention-controls.log`, 0.02 s, build 6m10s). Producer facts and the canonical signed-retention pin agree across activation, rejecting the short old promise when active; the actual immutable-contract planner rejects the short authorization and accepts the existing full policy obligation without changing its terms. Before activation the old pin remains exact. The current node/exporter/CLI rebuild is still running; a corrected three-node runtime result is not yet available.
- Funded v11 post-Final book transition is in progress (2026-10-18): added `ExecutionJobStateV11::adjudicate_held_final` for a genuine existing court/default after native authenticates its held unpaid fee and fixed signed H. It reallocates the original fee to requester refund, limits finder + burn to actual collected slash, changes no wire fields, and bounds both established/current judgment time by H. Added conservation/restore/duplicate controls for computation/source/default cases at 0/partial/full collection and immutable refusal controls for late/substituted/excess-slash inputs (`/tmp/misaka-rfc08-funded-held-final-book.log`, pending). This method moves no coins and is not yet wired to the native v11 held-fee/court/DA/retention journal; it does not complete 08-B or replace the ordinary-admission requirement in 08-C.

- Retention-corrected runtime build passed (2026-10-18, 14m05s, `/tmp/misaka-rfc12-consumer-retention-runtime-build.log`); ordinary CLI refund help passed without a stack override. Binary hashes are retained in `/tmp/misaka-rfc12-retention-rebuild-binaries.json`. A fresh same-policy three-node bootstrap is running with the unchanged monetary/resource/probe limits and 3,600 s workflow bound (`/tmp/misaka-rfc12-retention-fixed-bootstrap.log`, separate result path); the prior invariant-failure result is preserved. No corrected runtime acceptance result is available yet.
- Funded-v11 held-Final book controls **2/2 passed** (2026-10-18, `/tmp/misaka-rfc08-funded-held-final-book.log`, 0.02 s, build 8m04s): computation/source/default verdicts, zero/partial/full actual slash, original-fee conservation, restore, duplicate refusal and immutable substituted/late-input refusals. These fixtures exercise kernel accounting only; native original admission, held-fee authentication, retained collateral/quota/material and post-Final court/default wiring remain outstanding.

- Added the remaining funded-v11 clock/identity refusal controls (2026-10-18): established time before commit, current time before establishment, and zero finder are refused without book mutation; focused held-Final suite remains **2/2 passed**, 0.02 s (`/tmp/misaka-rfc08-funded-held-final-clock-controls.log`). Clarified the common sweep constant's documentation: it bounds payments, not inspected retained rows. No runtime code or consensus policy changes in this follow-up.

- Native39 post-Final resource audit (2026-10-18): Final credit previously set the shared quota subject `closed=true` even when its fee was held; the real protected-owner admission refuses every closed subject. Held Final now keeps that subject open and actual fee release/refund closes its existing rooted subject without changing quantities, attempt counts or retained-owner counts. The runtime release guard also requires a readable matching rooted subject. Expanded actual release/refund controls to verify closure preserves the rest of the row; compilation/run pending (`/tmp/misaka-rfc08-held-subject-controls.log`). This is still partial: source39's original obligation and class hold currently end at the signed original deadline, and its ordinary-answer debit refuses later time even though fee H includes `window_court`; paid-owner creation alone does not establish complete material/answer availability to H. The original signed resource authorization and any cross-activation funding/commit case must be handled coherently before 08-B is complete; no limit or authorization check was bypassed.

- Retention-corrected three-node run is terminal **FAIL** (2026-10-18): the first PoW block `dd87311a8ae561c5…` passed the earlier retention check, but A/B/JOIN all disqualified it with `paid consumer service cannot fit this class's whole minimum path`. Actual signed registration/binding carrier submission occurred; no accepted-chain inclusion is established. The existing private paid-v4 consumer service span is 1,000 DAA and cannot accommodate the inherited class's actual bind + derived receipt + challenge + 1 path. The owned controller was interrupted only after this replicated invariant failure; wrapper exit 254 records child SIGINT (-2), `/tmp/misaka-rfc12-retention-fixed-bootstrap-result.json` preserves FAIL, `/tmp/misaka-rfc12-retention-fixed-service-failure.json` preserves the cause, and all owned nodes are confirmed gone. The earlier failure and this run remain separate. No timer, class target, minimum-path guard or service interval has been changed to make the run pass; the private profile/contract inconsistency remains to be resolved before another runtime retry.

- Held-subject focused suite **11/11 passed** (2026-10-18, `/tmp/misaka-rfc08-held-subject-controls.log`, 0.03 s, build 6m20s). Release/refund controls now retain a real rooted subject projection and verify only `closed` changes: guarantees, consumed quantities, attempts and retained-owner count are not returned or reset. Public presets remain dormant. This proves the release/refund subject transition and compiles the held-Final credit branch; no new protected-owner-through-H admission or source-answer availability pass is claimed.

- RFC12 private service proposal prepared (2026-10-18): a standalone inspection linked to the current first-party core reads the exact inherited T12 clocks: bind 600, base receipt 600, challenge-at-0 120, court 3,000; minimum service lower bound **1,321 DAA** (`/tmp/misaka-rfc12-floor-clock-inspection.log`, compilation/runtime exit 0). Prepared a private-only factory change deriving `max(1,000, bind + base receipt + challenge + 1)` and keeping per-class longer-path refusal, both tails and public identity unchanged. The governance parameter register records old/new values, total material 6,000 → 6,321, evidence, fingerprint impact, pending test and **unapproved / not applied** status. The focused exact-policy/stale-mirror/public-refusal test is running; no modified-profile live run starts before the parameter approval required by the register. This proposal does not assert public security/economic approval.
- Native39 retention audit correction (2026-10-18): the job's class-hold row stores the original deadline, but action 72 reaps only `HOLD_CONSENT`; class/source release requires a zero live reference count, and `HOLD_JOB` is released by collection. The committed held-fee collection guard therefore retains the class and its source while a fee is held, even after the stored deadline. The earlier broad statement that the class itself expires at that deadline was too strong. The separate ordinary source-answer obligation still checks its original deadline and needs integration through H; no claim of full answer availability or full 08-B completion follows from the class-reference guard.

- Native39 source-H integration is in progress (2026-10-18): added separate native-only unit/answer readers that consume the same original prepaid reservation through an authenticated held-Final H without changing the stored binding/deadline, root, grant or wire. Legacy answer entry point still uses its original deadline. The caller derives H from the held native claim; the new reader requires H >= original deadline and H strictly inside the original quota interval. New liable Commit39 admission refuses an H outside that already reserved interval, rather than admitting an obligation that cannot be serviced. Public requests and cached producer answers call the new readers; existing slots, attempts, caller independence and conservation checks remain. Added rooted-budget boundary controls (old entry point rejects after deadline, new entry point consumes exact original reserve through H inclusive, refuses past H / shortened H / crossing interval); compilation/run pending (`/tmp/misaka-rfc08-source-horizon-controls.log`). These are accounting controls, not full original-funding/court/default end-to-end evidence. No arbitrary extra interval capacity or new signed authorization is fabricated.

- Private service proposal verification (2026-10-18): initial focused `kaspad --lib` test built successfully in 16m02s but its default test thread overflowed its stack and aborted (cargo exit 101/SIGABRT, preserved `/tmp/misaka-rfc12-private-service-derived-floor-tests.log`). The SAME compiled test binary with explicit `RUST_MIN_STACK=67108864` and one test thread passed **1/1**, 1.93 s (`/tmp/misaka-rfc12-private-service-derived-floor-stack64-controls.log`): exact derived 1,321 service, unchanged 3,000/2,000 tails, old 1,000 cannot fit the inherited minimum path, stale-value/mirror refusal and public armed-policy refusal. This host test stack setting changes no clocks, guard, quorum or lottery. Parameter approval is still pending; no modified-profile live run has started. The proposal is reviewable in the factory and governance register.
