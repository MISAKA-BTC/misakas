# ADR-0135 — A model is data: the permissionless registry derives its profile, proves its panel, and walks its lifecycle

* Status: **PROPOSED 2026-09-17; the rule set built in shadow the same day, and Protocol Upgrade A built behind a
  dormant fence the same evening** (`palw_model_registry_v1.rs`, `palw_state_v2.rs`; §7). No shipped preset arms
  it: no parameter or fingerprint moves until the operator schedules `palw_model_registry` at a height.
* Operator's direction, in the operator's words: "誰でもモデルを chain へ登録でき、検証時間・必要 Panel 数・claim 上限・
  報酬係数までプロトコルが自動計算し、条件を満たしたら自動 activate"; "モデルをコードではなくデータとして追加する";
  "実測値を consensus parameter にしない"; "Panel 側も permissionless — 自己申告だけではダメ"; "モデル登録から
  activation まで全部 state machine"; "share も最終的には手動設定をなくしたい"; "既存 Canonical ML VM で表現可能 →
  permissionless、新しい opcode が必要 → protocol upgrade".
* Supersedes, in ADR-0133: Decision 2's *measured* p99 as an input to the profile (measurements become telemetry),
  §9a's "profile freeze by the operator" and "tiny activation → ramp by the operator" (the lifecycle does both).
  Keeps everything else of ADR-0133 (three clocks, Little's law, class-local fail-closed, the artifact options).
* Builds on: [0133](0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md),
  [0132](0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md),
  [0131](0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) (economic compute from the
  graph), [0049](0049-palw-adjudication-contract.md) (the canonical IR the VM boundary is drawn on), [0067](0067-classes-are-chain-data-kernels-are-the-build.md)
  and [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) (the on-chain class registration and carriage this extends),
  [0056](0056-palw-permissionless-class-admission-and-share-economy.md) (class admission is permissionless already; this ADR removes the share it kept), [0071](0071-the-attempt-lanes-price-and-the-tickets-bound.md) (who may judge a class).

## 0. The sentence this ADR is

**A model is registered as data — a graph root, an artifact root and its bytes, a canonical job, a quantization,
a runtime version, and a bond — and from that data every node derives the same verification window, prefetch,
inflight cap, required ready seats, registration bond and claim cadence; seats count only when they prove the
artifact, their participation, their collateral and a recent verification; the class walks REGISTERED →
PREFETCHING → PROBATION → ACTIVE_LIMITED → ACTIVE on chain-visible facts and falls to HELD alone; and nothing —
not a window, not a rate, not a share — is ever stated by a registrant, measured by an operator or committed to
a source tree.**

## 1. What today's path still needs a human for, and what stays a human's

Today (ADR-0133 §9a): shadow measurement → the operator freezes a profile → a fence in `main` → a share set by
hand → a release. Each step is a fork or a decision per model, and the p99 the operator would freeze is the
measuring host's (an M4 200 s, an H100 50 s, a slow disk 500 s). This ADR removes every one of them **for a model
the canonical ML VM already expresses** (matmul, attention, the mixture router and experts, the gated-delta
recurrence, norms, rotations, the LM head — ADR-0049's IR at the class's `runtime_version`). What stays a
protocol upgrade: a model that needs an instruction the VM lacks. That is not a model registration but a VM
instruction-set change, and `palw_manifest_verdict_v1` says so (`UnsupportedOp`) rather than letting a node
guess at an op's meaning.

## 2. Decisions

**Decision 1 — the manifest.** `PalwModelManifestV1 { graph_ir_root, artifact_root, artifact_bytes,
canonical_prefill_tokens, canonical_decode_tokens, quantization_format, runtime_version }` with a registration
bond. It is today's on-chain class registration (ADR-0067's carriage: the profile IR and the canonical job)
plus the artifact's bytes — and nothing a human decides: no window, no reward, no share. A manifest is judged
before it is a class: `Valid`, `UnsupportedOp` (the VM boundary), `EmptyArtifact`, `EmptyJob`, `ZeroWork`.

**Decision 2 — the work, derived.** From the graph, every node computes `PalwModelWorkV1 { verification_ccu
(ADR-0131's economic compute of the job a seat replays), economic_ccu_per_claim (the same over the class's
expected draws, ADR-0132), artifact_bytes, working_set_bytes, ops_supported }`. Dense MACs, attention over the
kv length, the recurrence, the active experts, the logits and the elementwise work are counted by the one
versioned cost table; two nodes with one graph get one number.

**Decision 3 — the profile, derived, never measured.** Against `PalwRegistryGlobalsV1` — one set of reference
constants for every class, changed only by a fence:

* `verification_window_spans = ⌈safety × verification_ccu / reference_work_per_span⌉ + receipt_allowance_spans`
* `artifact_prefetch_spans = ⌈io_safety × artifact_bytes / reference_bytes_per_span⌉`
* `required_ready_seats = max(seat_count + spare_seats, ⌈seat_count × verification_ccu / (utilization ×
  reference_work_per_span)⌉)` — the panel plus the spare, or more where one claim a span already needs more
  replay time than that many seats offer at the target utilization
* `max_inflight_claims = ⌊utilization × required_ready_seats × window × reference_work_per_span / (seat_count ×
  verification_ccu)⌋` — Little's law
* `registration_bond = bond_per_span × verification_window_spans` — the burden a class puts on panels is what
  its registrant bonds
* `admission_claims_per_span = min(budget_ccu_per_span / economic_ccu_per_claim, max_inflight / window)` —
  Decision 5

The fleet's reference (ADR-0133): 4 G MAC-eq/s (2.4 T a span), 1 GB/s (600 GB a span), ×2 and ×2, one span of
receipt allowance, five seats plus two spare, 70 %. On it the dense tier and the hybrid derive two spans (one of
verification, one of allowance), one span of prefetch, seven ready seats; a Kimi-class of 1 T MAC-eq and 300 GiB
derives two spans, two of prefetch, seven seats; a 10 T MAC-eq graph derives ten spans and more than seven seats
— all from the same code, none from a decision. **Measured p95/p99 are telemetry**: op 185 keeps printing them
so the fleet can see whether the derived window is being met; they enter no rule.

**Decision 4 — readiness is evidence.** `PalwReadinessEvidenceV1 { artifact_root_matches, all_chunks_held,
participation_ok, free_collateral_sompi, last_probe_ok_span }`; a seat is ready for a class only with the root
matched, every chunk held (a possession proof over the manifest's chunks), its node participating (synced, not
held), free collateral for `readiness_collateral_multiple` seat exposures, and a canonical probe verified within
`readiness_probe_max_age_spans`. A declaration alone counts for nothing (the failure ADR-0132 found: every
genesis bond "declared" the hybrid and one could run it). A seat that says `Incapable` is simply not ready and
is not dealt the class until its evidence says otherwise.

**Decision 5 — no share.** A class's claim cadence is `budget_ccu_per_span / economic_ccu_per_claim`, capped by
what its window can hold: a heavy model claims rarely and is paid much a claim, a light one often and little, and
the network's compute budget a span is what is constant. Under the single lottery (ADR-0132 S, ADR-0133 §9a Fence
2) each class's share of draws is its admission over every class's (`palw_class_shares_from_admission_v1`); a
new class takes its slice from all in proportion, a class with zero admission holds none. The reward stays
ADR-0132's `f(EconomicAttempted, GlobalRate)` — no model's name enters it — and a per-claim escrow cap is not
adjusted per model: cadence is what moves.

**Decision 6 — the lifecycle.**

```text
REGISTERED ──manifest valid──▶ PREFETCHING ──ready seats ≥ required, collateral ok──▶ PROBATION
PROBATION ──probation_claims probes passed, none failed──▶ ACTIVE_LIMITED (a tenth of admission)
ACTIVE_LIMITED ──stable_epochs spans at target, nothing held──▶ ACTIVE (full admission)
PROBATION / ACTIVE_LIMITED / ACTIVE ──panel not drawable (ready < seat_count) or utilization ≥ 1──▶ HELD
HELD ──ready seats ≥ required, utilization < 1──▶ PROBATION (never straight back to ACTIVE)
```

Every arrow is `palw_lifecycle_step_v1` over `PalwLifecycleObservationV1` (ready seats, probes passed and
failed, utilization, collateral, span stability) — chain-visible facts at a span boundary. `HELD` holds the
class's own new claims and nothing else: another class on its own facts stays where it is, the execution lane
schedules the classes with `Final`s, the anchor cadence is untouched (pinned).

**Decision 7 — the boundary.** Registration is permissionless for a graph the canonical VM expresses at the
manifest's `runtime_version`; a graph that needs a new op is refused as `UnsupportedOp` and its op is a protocol
upgrade. The registry therefore never has to understand an operation it was not built to verify.

## 3. What Kimi's registration looks like

A third party submits the manifest and the bond. Every node derives `economic_ccu`, `verification_ccu`,
`artifact 287 GiB`, `prefetch 2`, `verification 2`, `max inflight`, `required ready 7` (or more), the bond. Seven
operators fetch the artifact, run the probe, and their evidence makes them ready. Ten probe claims finalize
with none failing: limited activation at a tenth of the derived admission. Three stable spans: `ACTIVE`. Two
operators leave: `HELD`; the dense tier and the lane continue; the operators return: `PROBATION` again. Nobody
updates a repository.

## 4. What this changes in ADR-0132 and ADR-0133

* ADR-0133 Decision 2 (`warm_p99`/`cold_p99` as inputs): **superseded** — the shadow module keeps them as this
  node's telemetry, the registry's profile ignores them.
* ADR-0133 §9a Fence 1: **becomes Protocol Upgrade A**, the generic framework — manifest, derived profile,
  readiness evidence, lifecycle, class-local capacity gate — armed once; no "Kimi activation" step exists after it.
* ADR-0132 Decision 6 (a new model earns only after a shadow period): **becomes** the lifecycle's `PROBATION` and
  `ACTIVE_LIMITED`, automatic.
* ADR-0124 Decision 6's unit and ADR-0131's rate: **the rate is global** (ADR-0132 C); no class sets a unit.

## 5. Security amendments

* **SA-1 — the registrant cannot shrink its window**: the profile reads the graph, not the manifest's opinion
  of it; under-reporting is impossible because nothing is reported.
* **SA-2 — the registrant cannot flood**: the registration bond grows with the window it imposes, admission is
  budgeted, and a class that cannot draw a panel is `HELD` at no cost to any other.
* **SA-3 — a seat cannot fake readiness cheaply**: the possession proof is over the artifact root the manifest
  names, the probe is a canonical verification the chain can check, and the collateral is real.
* **SA-4 — a class cannot fork the network by existing**: a manifest the VM cannot express is refused at
  registration; nothing about a registered class changes a rule any node runs.
* **SA-5 — globals are one fence, not one per model**: the reference constants are consensus parameters a
  fence moves for every class at once; a measured p99 never moves one.

## 6. The road

```text
now      ADR-0132 / 0133 / 0135 in shadow: the CLI prints each class's derived profile beside its telemetry
   A     Protocol Upgrade A — Permissionless Model Registry V1: manifest, derived profile, readiness evidence,
         lifecycle, class-local capacity gate (ADR-0132 F1/F2 folded in); the dense tier and the hybrid walk it live
   B     the single lottery, if adopted (ADR-0132 S): class shares from admission
   C     EconomicAttempted + a global rate + budgeted admission (ADR-0132 Fence 3), no per-model cap
         — built 2026-09-17 behind the dormant `palw_economic_payout` fence (ADR-0132 §7); armed after A's drill
after    Kimi, Llama, a new mixture, a 100B model: a manifest and a bond, no fork
```

## 7. What is built — Protocol Upgrade A, behind a dormant fence (2026-09-17)

**Amended 2026-09-17 (evening), found by the devnet drill:** the fold's registry input named "the genesis
classes' work, from the registrations the bundle carries" — but no shipped bundle registration carries an
admission carriage (a genesis class's profile is the catalog's, of which the bundle holds only a root), so no
genesis class had a work, no row opened for the floor or for testnet-11's three classes, and a class node-1
registered before the fence had no row either (its carriage was consumed by the registration block). The
node now describes a class from what it can prove it computes: a genesis class through the canonical class
table the binary compiles (`canonical_classes_v1`, the same derivation the registration message uses), a
registered class through the carriage the chain carried — before or after the fence — kept in the class
carriage store and adopted by a syncing node before the block that needs it. A class the node cannot
describe stays a legacy row. The fold itself is unchanged: it opens rows from the works it is handed.

**Amended the same evening, the second drill finding:** a possession proof names the span it was made for
and the fold took it only in that span or the next; on the devnet's two-DAA spans the carrier that brings it
waited longer than that in mempools and for a block (every proof refused as "names span 11 at span 21").
A proof now lands within `PALW_READINESS_LANDING_SPANS_V1 = 8` spans of the span it names (a future span
is still refused), and the row it writes is dated at the named span's first DAA: a late proof is exactly as
fresh as when it was made, a replayed one renews nothing, and the thirty-span readiness age bounds the rest; on testnet-11 (five-DAA spans, ~5 DAA an
hour) the allowance is about eight hours of carrier latency.

**The fence.** `Params::palw_model_registry: Option<ForkActivation>` — `None` on every shipped preset
(the t11 fingerprint `135b6ee0…` does not move); hashed `Some`-only; the fork-id gate names it when
armed; `validate_palw_v2` refuses it without `palw_panel_economy` and `palw_execution_lane` at or
below its height (it reads seat exposure and steps at the lane's span boundaries). Arming it is a
height in one constant; the pins are `adr0135_the_registry_fence_is_dormant_everywhere_and_arms_by_height`.

**On chain, past the fence** (`palw_state_v2.rs`, ADR-0135's rows in their own guarded root
sub-block and carriage tail `0xAA`, delta entries 51/52):

* `model_lifecycles: class → PalwModelLifecycleRowV1 { state, work, profile, since_span, probes, the
  last boundary's reading, admission }` and `seat_readiness: (bond, class) → PalwSeatReadinessRowV1
  { proved_daa, proved_span, leaf_index }`.
* **Decision 1–3 — the profile from the graph.** `ClassRegistered` with its carriage opens the row:
  `palw_model_work_from_carriage_v1(profile, canonical)` reads the verification compute and the
  draw compute off the graph with ADR-0131's cost table (the artifact bytes are an estimate from the
  dense weights until a manifest carries them — V2); the class starts `PREFETCHING`, or `REGISTERED`
  where the graph derives no work (the VM boundary). The classes registered before the fence get
  their work from the bundle's genesis registrations (`PalwModelRegistryFoldV1::genesis_works`, the
  same on every node because the bundle is fingerprinted); a class registered before the fence
  without a carriage keeps no row and is never gated. The profile is re-derived at every boundary
  from the stored work and the class's live target (`palw_lifecycle_profile_v1`).
* **Decision 4 — readiness is a possession proof.** `SeatReadinessProved { bond, class_id, span,
  opening, signature }`: one leaf of the registered artifact root, at an index inside the window the
  (class, bond, span) challenge names (`palw_readiness_challenge_seed_v1`, eight consecutive leaves),
  the span current or just closed, signed by the bond's key (checked at acceptance like a
  capability declaration; below the fence refused). A seat is ready for a class while its proof is
  younger than 30 spans, it is active and above the floor, and its free collateral covers three
  times the network's floor (`palw_model_registry_ready_seats_v1`). The probe half is the class's
  own claims: a `Final` passes, a court fraud or a withholding fails (`note_model_probe`).
* **Decision 6 — the lifecycle at the boundary.** `step_model_registry` at every span boundary
  (before the lane's rotation): rows open, every row is observed (ready seats, claims in flight,
  the span's probes, utilization = inflight × seats / (ready × window)) and stepped by
  `palw_lifecycle_step_v1`; the base class is `ACTIVE` and never gated.
* **The class-local gate** (ADR-0132 F1/F2). `apply_attempt` refuses a claim of a class whose row
  does not admit (`ClassNotAdmitting`) or is at its inflight cap (`ClassInflightCapped`); a bind
  window that closes with fewer ready seats than a panel voids as `NoCapablePanel` (void reason 4)
  rather than `BindTimeout`; the draw and the fold's panel validation judge by evidence
  (`palw_bond_may_judge_class_v4`, `PalwPanelDrawPolicyV1::readiness`): under the registry a seat
  needs a fresh proof, not a declaration, and the base class stays open to every bond.
* **Decision 5 — shares from admission.** At every boundary the shares of the rowed classes are
  written from `admission_claims_per_span × admission_permille(state)` over the keys the table
  already holds — the base class holding what is left and never below its floor, a class
  registered before the fence without a row keeping the share it has. `PROBATION` admits at a twentieth (a class must be able to produce the claims that probe it —
at zero it could never leave probation), `ACTIVE_LIMITED` at a tenth, `ACTIVE` in full; a class that
admits anything holds at least the grant floor of one permille, so a target exists for it.

**Read.** Op 186 `getPalwModelRegistry` (`misaka palw registry`): the fence, the globals, every
class's row and its reading now, every proof and whether it is fresh.

**Tests** (`palw_state_v2::tests::adr0135`, `palw_model_registry_v1::tests`, the fence pin): the
boundary opens rows from the genesis work and the base class is active (and the delta reverts, the
carriage round-trips, the root moves only with rows, the fold without the fence is byte-identical);
readiness is a possession proof — a wrong leaf, a forged leaf, a stale span and the dormant fence
are refused, six ready seats leave a class prefetching and the seventh admits it to probation, a
thin bond does not count; a held class takes no claims while the base class keeps producing; the
inflight cap refuses the claim after the cap; `NoCapablePanel` names the void and the class recovers
through probation, and an operator outage (every proof aged out) holds it alone; the discriminant
pins (void reason 4, the object as the enum's last variant, delta entries 51 and 52).

**The seat's side, built the same night.** The panel service submits `SeatReadinessProved` on its
own (`readiness_duties`): every thirty seconds it reads op 186; for each non-base class the
registry is in force for it proves when the chain holds none of this bond's proofs for the class
or the one it holds is past half the readiness age (`palw_readiness_duty_due_v1` — a restart
re-reads the chain and never re-sends a fresh proof), never twice in a span; the leaf is the first
under the opening cap inside the challenge's window, opened from the held artifact
(`PalwExecutionBackendV1::artifact_row_opening`; every family roots its artifact in one
streaming pass and opens one row by a second — the inventory is never materialised: eight nodes
that materialised the A16 inventory to root it rebooted a 24 GiB host on 2026-09-17), rooted locally against the class's registered root
before it is signed by the bond's key. **Fail-closed**: a class this node holds no artifact for, or
holds under a different root, gets no proof and is named once in the log; a node in IBD or not near
the tip proves nothing; a bond that is inactive, below the floor or without the readiness multiple of
free collateral (op 186's `bonds`) proves nothing and says why once. A reorg that drops a proof's
block leaves no row, and the next span's duty re-proves against the current challenge — an old
span's proof is never resent (the fold refuses it). The producer reads the
same op before a draw and holds (a log line, no rule) for a class the registry holds or has at its
inflight cap. `--palw-model-registry-devnet=<daa>` arms the registry on a private devnet (with the
panel economy it reads, where the devnet has none). The registry's seat count is the network's
panel size (the bundle's panel params: five on testnet-11, so seven ready seats; the global
constant is the fallback), never a number of its own. The drill is
`scripts/misaka-palw-model-registry-devnet-drill.sh`: eight fixture nodes, the lane at 2-DAA spans,
the registry armed at DAA 20, an artifact every node holds (`CLASS_ARTIFACT=`) so the seats prove
for its genesis class; it waits for the fence, the rows, the proofs, the grace's end with the base
class ACTIVE and the chain producing, and a restarted node's rows — the record is §7's last entry.

**The activation grace.** Proofs are refused below the fence, so at the fence no seat is ready and
every live class would be HELD at the first boundary. The fold therefore opens rows and takes
proofs from the fence but steps no row, moves no share, judges no draw by evidence and names no
`NoCapablePanel` until the fence is one readiness age old (`PalwModelRegistryFoldV1::grace_until_daa`,
30 spans); the classes registered before the fence open in the state their history earns (ACTIVE
with a `Final`, PREFETCHING without) and are governed from the grace's end. A test walks it.

**What the review asked, answered.**

* *Migration at activation*: nothing is migrated — the rows are derived at the first boundary from
  the chain (the base class ACTIVE; a class with a `Final` ACTIVE; a class with work and no `Final`
  PREFETCHING; a class the fence found without a carriage, none — never gated); the work comes
  from the bundle's genesis registrations, the same on every node.
* *Determinism*: the profile, the observation and the step read the state, the block context and
  `PalwTransitionExtrasV1` built from `Params` — no clock, no local store, no RPC; the fold test
  folds one block twice and compares roots, and the delta reverts to the parent.
* *Expiry*: a proof counts for 30 spans of DAA and no longer; a bond that leaves `Active`, drops
  below the floor or loses its collateral headroom stops counting at the next reading whatever its
  proof says; a class's artifact is its registration — a new root is a new class and new rows.
* *Inflight*: never a counter. Claims in flight are counted from the claims at every reading
  (accepted, not terminal), so a `Final`, a void, a timeout or a reorg needs no decrement.
* *NoCapablePanel*: named once per claim when its bind window closes without a panel; a HELD class
  takes no new claim, so nothing loops; with zero eligible seats the claim voids and the class holds.
* *Cadence*: shares move on the first block of a span (the lane's `opens_span`); a class enters the
  lottery when its share is above zero — ACTIVE_LIMITED or ACTIVE — and its share key must already
  exist (the registration grants it).
* *Old nodes*: a build without the fence cannot decode `SeatReadinessProved`, and past the fence
  computes no rows and no reasons — but it never gets there: the fork-id gate names the height, so
  it is refused as a peer from the fence; below the fence the new build refuses the object, so no
  block carries one.
* *Observability*: op 186 prints each class's state with its reason (`held: ready 3 < 5 for a
  panel …`, `probing 4/10 …`, `base class …`), `since_span`, the derived profile, ready seats and
  claims in flight now against the cap, utilization, admission, `no_capable_panel_voids`, the
  counts of classes by state, the bonds with headroom for a seat, and each proof's freshness with
  the reason it does not count (`stale`, `bond inactive`, `below floor`, `collateral short`) — so
  a HELD by the rule and a stop by a fault read differently.

**`artifact_bytes` is not consensus-critical.** It is an estimate from the graph's dense weights and
feeds the prefetch allowance only; admission, the bond, the window and the inflight cap read the
compute. No rule may start reading it before a manifest carries the value (V2).

**Not built, stated.** The manifest's own `artifact_bytes` and a manifest transaction distinct from
today's `ClassRegistered`; the possession proof's width (one leaf a proof, V1); a held class's
existing claims run to their end untouched. The PALW state sync path (`PalwStateSyncV2`, unused by
the live node) carries no lane and no registry.

## 8. Number hygiene

0135 was free when written; the next free number is 0136.
